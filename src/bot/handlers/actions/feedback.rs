//! Обратная связь: сообщения пользователей администраторам и ответы администраторов.
//!
//! Сообщения любого типа (текст, фото, файлы, голосовые) доставляются через `copy_message`.
//! Альбом приходит отдельными апдейтами с общим `media_group_id`: первая часть открывает
//! заголовок, остальные части того же альбома копируются следом без повторного заголовка.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::bot::handlers::shared::HandlerResult;

use crate::bot::handlers::state::{
    BotState, WizardState, clear_wizard_state, sender_display_name, set_wizard_state,
};
use teloxide::payloads::{CopyMessageSetters, SendMessageSetters};
use teloxide::prelude::{Bot, ChatId, Message, Requester};
use teloxide::types::{InlineKeyboardMarkup, ReplyParameters};

/// Пауза между сообщениями одного пользователя администраторам. Каждое сообщение уходит
/// всем админам, поэтому без паузы бот и Mini App позволяют засыпать их чаты.
const FEEDBACK_COOLDOWN: Duration = Duration::from_secs(60);

/// Время последнего сообщения обратной связи по пользователям; общее для бота и Mini App.
static LAST_FEEDBACK: LazyLock<Mutex<HashMap<i64, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Отмечает сообщение `user_id` в `now`, если пауза прошла. `Err` — сколько ещё ждать.
fn claim_slot(
    sent: &mut HashMap<i64, Instant>,
    user_id: i64,
    now: Instant,
    cooldown: Duration,
) -> Result<(), Duration> {
    sent.retain(|_, at| now.duration_since(*at) < cooldown);
    if let Some(at) = sent.get(&user_id) {
        return Err(cooldown - now.duration_since(*at));
    }
    sent.insert(user_id, now);
    Ok(())
}

/// Занимает слот обратной связи пользователя. `Err` — сколько секунд ещё ждать.
pub fn claim_feedback_slot(user_id: i64) -> Result<(), u64> {
    let mut sent = LAST_FEEDBACK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    claim_slot(&mut sent, user_id, Instant::now(), FEEDBACK_COOLDOWN)
        .map_err(|wait| wait.as_secs().max(1))
}

/// Освобождает слот, если сообщение не дошло ни до одного администратора.
pub fn release_feedback_slot(user_id: i64) {
    let mut sent = LAST_FEEDBACK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    sent.remove(&user_id);
}

/// Текст для пользователя, которому нужно подождать перед следующим сообщением.
pub fn feedback_cooldown_text(wait_secs: u64) -> String {
    format!("Следующее сообщение администратору можно отправить через {wait_secs} с.")
}

/// Как обработать входящее сообщение в режиме обратной связи.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackStep {
    /// Первое сообщение: с заголовком и подтверждением отправителю.
    First,
    /// Следующая часть уже отправленного альбома.
    AlbumPart,
    /// Сообщение не относится к обратной связи: режим завершён.
    Leave,
}

/// `awaited_group` — альбом, часть которого уже отправлена; `message_group` — альбом сообщения.
pub fn feedback_step(awaited_group: Option<&str>, message_group: Option<&str>) -> FeedbackStep {
    match (awaited_group, message_group) {
        (None, _) => FeedbackStep::First,
        (Some(awaited), Some(current)) if awaited == current => FeedbackStep::AlbumPart,
        _ => FeedbackStep::Leave,
    }
}

fn message_group(msg: &Message) -> Option<String> {
    msg.media_group_id().map(|group| group.0.clone())
}

/// Копирует сообщение в чат `to`; при заданном `header` сначала отправляет заголовок
/// с клавиатурой, а копию — ответом на него.
async fn copy_with_header(
    bot: &Bot,
    to: ChatId,
    msg: &Message,
    header: Option<&str>,
    keyboard: InlineKeyboardMarkup,
) -> Result<(), teloxide::RequestError> {
    let mut copy = bot.copy_message(to, msg.chat.id, msg.id);
    if let Some(header) = header {
        let header_msg = bot.send_message(to, header).reply_markup(keyboard).await?;
        copy = copy.reply_parameters(ReplyParameters::new(header_msg.id));
    }
    copy.await?;
    Ok(())
}

/// Оставляет wizard открытым для остальных частей альбома или завершает его.
async fn continue_album_or_clear(
    state: &BotState,
    user_id: i64,
    next: Option<WizardState>,
) -> Result<(), anyhow::Error> {
    match next {
        Some(next) => set_wizard_state(state, user_id, next).await,
        None => clear_wizard_state(state, user_id).await,
    }
}

/// Передаёт сообщение пользователя всем администраторам с кнопкой «Ответить».
pub async fn send_user_feedback(
    bot: &Bot,
    msg: &Message,
    state: &BotState,
    user_id: i64,
    step: FeedbackStep,
) -> HandlerResult {
    if step == FeedbackStep::First
        && let Err(wait_secs) = claim_feedback_slot(user_id)
    {
        bot.send_message(msg.chat.id, feedback_cooldown_text(wait_secs))
            .await?;
        return Ok(());
    }
    let header = (step == FeedbackStep::First).then(|| {
        let username = msg.from.as_ref().and_then(|user| user.username.as_deref());
        state.config.bot_messages.admin_feedback_text(
            user_id,
            username,
            sender_display_name(msg).as_deref(),
        )
    });
    let keyboard = crate::bot::keyboards::feedback_reply_keyboard(user_id);

    let mut delivered = 0usize;
    for admin_id in &state.config.admin_ids {
        match copy_with_header(
            bot,
            ChatId(*admin_id),
            msg,
            header.as_deref(),
            keyboard.clone(),
        )
        .await
        {
            Ok(()) => delivered += 1,
            Err(error) => tracing::warn!(
                admin_id = *admin_id,
                user_id = user_id,
                error = %error,
                "Failed to deliver feedback to admin"
            ),
        }
    }

    let next = message_group(msg).map(|group| WizardState::UserFeedback {
        media_group_id: Some(group),
    });
    continue_album_or_clear(state, user_id, next).await?;

    if step == FeedbackStep::First {
        tracing::info!(
            user_id = user_id,
            delivered = delivered,
            "Feedback forwarded to admins"
        );
        let text = if delivered > 0 {
            state.config.bot_messages.feedback_sent_or_default()
        } else {
            release_feedback_slot(user_id);
            "Не удалось передать сообщение администратору. Попробуйте позже."
        };
        bot.send_message(msg.chat.id, text).await?;
    }
    Ok(())
}

/// Доставляет ответ администратора пользователю с кнопкой продолжить переписку.
pub async fn send_feedback_reply(
    bot: &Bot,
    msg: &Message,
    state: &BotState,
    admin_id: i64,
    tg_user_id: i64,
    step: FeedbackStep,
) -> HandlerResult {
    let header = (step == FeedbackStep::First)
        .then(|| state.config.bot_messages.feedback_reply_header_or_default());
    let result = copy_with_header(
        bot,
        ChatId(tg_user_id),
        msg,
        header,
        crate::bot::keyboards::feedback_continue_keyboard(),
    )
    .await;

    match result {
        Ok(()) => {
            let next = message_group(msg).map(|group| WizardState::AdminFeedbackReply {
                tg_user_id,
                media_group_id: Some(group),
            });
            continue_album_or_clear(state, admin_id, next).await?;
            if step == FeedbackStep::First {
                tracing::info!(
                    admin_id = admin_id,
                    tg_user_id = tg_user_id,
                    "Feedback reply delivered"
                );
                bot.send_message(
                    msg.chat.id,
                    format!("Ответ отправлен пользователю {tg_user_id}."),
                )
                .await?;
            }
        }
        Err(error) => {
            clear_wizard_state(state, admin_id).await?;
            tracing::warn!(
                admin_id = admin_id,
                tg_user_id = tg_user_id,
                error = %error,
                "Failed to deliver feedback reply"
            );
            bot.send_message(
                msg.chat.id,
                format!("Не удалось доставить ответ пользователю {tg_user_id}: {error}"),
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{FeedbackStep, claim_slot, feedback_step};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn feedback_slot_waits_for_cooldown() {
        let cooldown = Duration::from_secs(60);
        let start = Instant::now();
        let mut sent = HashMap::new();
        assert_eq!(claim_slot(&mut sent, 1, start, cooldown), Ok(()));
        assert_eq!(
            claim_slot(&mut sent, 1, start + Duration::from_secs(20), cooldown),
            Err(Duration::from_secs(40))
        );
        assert_eq!(
            claim_slot(&mut sent, 2, start + Duration::from_secs(20), cooldown),
            Ok(())
        );
        assert_eq!(claim_slot(&mut sent, 1, start + cooldown, cooldown), Ok(()));
    }

    #[test]
    fn feedback_slots_of_quiet_users_are_dropped() {
        let cooldown = Duration::from_secs(60);
        let start = Instant::now();
        let mut sent = HashMap::new();
        claim_slot(&mut sent, 1, start, cooldown).unwrap();
        claim_slot(&mut sent, 2, start + cooldown, cooldown).unwrap();
        assert_eq!(sent.len(), 1);
    }

    #[test]
    fn first_message_is_accepted_with_or_without_album() {
        assert_eq!(feedback_step(None, None), FeedbackStep::First);
        assert_eq!(feedback_step(None, Some("1")), FeedbackStep::First);
    }

    #[test]
    fn only_same_album_continues_feedback() {
        assert_eq!(feedback_step(Some("1"), Some("1")), FeedbackStep::AlbumPart);
        assert_eq!(feedback_step(Some("1"), Some("2")), FeedbackStep::Leave);
        assert_eq!(feedback_step(Some("1"), None), FeedbackStep::Leave);
    }
}
