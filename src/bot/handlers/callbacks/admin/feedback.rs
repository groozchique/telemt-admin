use super::super::common::{ack_callback, admin_callback_target, replace_wizard_state};
use super::AdminActionResult;
use crate::bot::handlers::callback_data::CallbackAction;
use crate::bot::handlers::state::{BotState, WizardState};
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::{Bot, CallbackQuery, Requester};

pub async fn handle(
    bot: &Bot,
    q: &CallbackQuery,
    state: &BotState,
    action: CallbackAction,
) -> AdminActionResult {
    let CallbackAction::PromptFeedbackReply { tg_user_id } = action else {
        return Ok(false);
    };
    let Some((admin_id, chat_id, _)) = admin_callback_target(bot, q, state).await? else {
        return Ok(true);
    };
    replace_wizard_state(
        state,
        admin_id,
        WizardState::AdminFeedbackReply {
            tg_user_id,
            media_group_id: None,
        },
    )
    .await?;
    ack_callback(
        bot,
        q.id.clone(),
        Some("Жду ответ следующим сообщением"),
        false,
    )
    .await?;
    bot.send_message(
        chat_id,
        format!(
            "Ответ пользователю {tg_user_id}. Отправьте его следующим сообщением: текст, фото, видео, файл или голосовое."
        ),
    )
    .reply_markup(crate::bot::keyboards::cancel_keyboard(
        CallbackAction::ShowAdminHome,
    ))
    .await?;
    Ok(true)
}
