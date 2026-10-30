//! Уведомления пользователям о сроке доступа и квоте трафика.
//!
//! Фоновая задача раз в `user_notifications.check_interval_secs` читает список пользователей
//! telemt и пишет пользователям `tg_<id>`: заранее о скором окончании срока, об истечении
//! срока и о расходе квоты. Каждое уведомление отправляется один раз: отметка хранится
//! в SQLite и привязана к значению срока или квоты, поэтому после продления
//! уведомления приходят снова. Когда расход трафика опускается ниже порога (сброс
//! счётчика), отметка снимается, и при следующем превышении предупреждение придёт снова.

use crate::bot::handlers::BotState;
use crate::bot::handlers::format::format_bytes_human;
use crate::bot::handlers::tg_id_from_telemt_username;
use crate::config::UserNotificationsConfig;
use crate::telemt_backend::{TelemtUserInfo, TelemtUserLimits};
use chrono::{DateTime, Utc};
use teloxide::RequestError;
use teloxide::prelude::{Bot, ChatId, Requester};
use tokio::time::{Duration, MissedTickBehavior};

const SECS_PER_DAY: i64 = 86_400;
/// Об истечении давнее не сообщаем: иначе после включения функции уведомление
/// получат все, у кого срок когда-то кончился.
const EXPIRED_NOTIFY_WINDOW_SECS: i64 = 7 * SECS_PER_DAY;
/// Пауза между сообщениями, чтобы не упираться в лимиты Telegram.
const SEND_PAUSE: Duration = Duration::from_millis(50);

/// Срок и квота пользователя в удобном для расчётов виде.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessLimits {
    pub expires_at: Option<DateTime<Utc>>,
    /// `None` — квота не задана (0 тоже считается «без квоты»).
    pub quota_bytes: Option<u64>,
    pub used_bytes: u64,
}

impl AccessLimits {
    pub fn new(
        expiration_rfc3339: Option<&str>,
        quota_bytes: Option<u64>,
        used_bytes: u64,
    ) -> Self {
        Self {
            expires_at: expiration_rfc3339
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            quota_bytes: quota_bytes.filter(|q| *q > 0),
            used_bytes,
        }
    }

    pub fn from_user_info(info: &TelemtUserInfo) -> Self {
        Self::new(
            info.expiration_rfc3339.as_deref(),
            info.data_quota_bytes,
            info.total_octets.unwrap_or(0),
        )
    }

    /// Сколько дней осталось, с округлением вверх; 0 — срок уже истёк.
    pub fn days_left(&self, now: DateTime<Utc>) -> Option<i64> {
        let remaining = (self.expires_at? - now).num_seconds();
        Some(if remaining <= 0 {
            0
        } else {
            (remaining + SECS_PER_DAY - 1) / SECS_PER_DAY
        })
    }

    /// Плейсхолдеры `{expires_at}`, `{days_left}`, `{used}`, `{quota}` для текстов.
    pub fn template_vars(&self, now: DateTime<Utc>) -> Vec<(&'static str, String)> {
        vec![
            (
                "expires_at",
                self.expires_at.map(format_expires_at).unwrap_or_default(),
            ),
            (
                "days_left",
                self.days_left(now)
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            ),
            ("used", format_bytes_human(self.used_bytes)),
            (
                "quota",
                self.quota_bytes.map(format_bytes_human).unwrap_or_default(),
            ),
        ]
    }

    /// Строки о сроке и трафике под ссылкой; `None`, если ни срок, ни квота не заданы.
    pub fn status_lines(&self, now: DateTime<Utc>) -> Option<String> {
        let mut lines = Vec::new();
        if let (Some(expires_at), Some(days_left)) = (self.expires_at, self.days_left(now)) {
            lines.push(if days_left > 0 {
                format!(
                    "⏳ Доступ до {} (осталось дней: {days_left})",
                    format_expires_at(expires_at)
                )
            } else {
                format!("⛔ Срок доступа истёк {}", format_expires_at(expires_at))
            });
        }
        if let Some(quota) = self.quota_bytes {
            lines.push(format!(
                "📊 Трафик: {} из {}",
                format_bytes_human(self.used_bytes),
                format_bytes_human(quota)
            ));
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

fn format_expires_at(dt: DateTime<Utc>) -> String {
    dt.format("%d.%m.%Y %H:%M UTC").to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// Срок закончится не позже чем через столько дней.
    ExpiryWarn(u32),
    Expired,
    /// Израсходовано не меньше стольких процентов квоты.
    QuotaWarn(u8),
}

impl NoticeKind {
    fn key(self) -> String {
        match self {
            Self::ExpiryWarn(days) => format!("expiry_warn_{days}"),
            Self::Expired => "expired".to_string(),
            Self::QuotaWarn(percent) => format!("quota_warn_{percent}"),
        }
    }
}

/// Уведомление к отправке. Если пройдено сразу несколько порогов, отправляется одно
/// сообщение по самому близкому, а остальные (`also_mark`) только отмечаются,
/// чтобы не прислать их следующим тиком.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedNotice {
    pub tg_user_id: i64,
    pub kind: NoticeKind,
    pub also_mark: Vec<NoticeKind>,
    /// Срок или квота, к которой относится отметка об отправке.
    pub scope: String,
    pub limits: AccessLimits,
}

/// Пороги квоты, которые расход сейчас не превышает: их отметки снимаются, чтобы после
/// сброса счётчика трафика предупреждение пришло снова. Пусто, если квота не задана.
pub fn uncrossed_quota_notices(
    user: &TelemtUserLimits,
    config: &UserNotificationsConfig,
) -> Vec<NoticeKind> {
    let Some(quota) = user.data_quota_bytes.filter(|q| *q > 0) else {
        return Vec::new();
    };
    let used_percent = u128::from(user.total_octets) * 100 / u128::from(quota);
    let mut uncrossed: Vec<u8> = config
        .quota_warn_percent
        .iter()
        .copied()
        .filter(|percent| used_percent < u128::from(*percent))
        .collect();
    uncrossed.sort_unstable();
    uncrossed.dedup();
    uncrossed.into_iter().map(NoticeKind::QuotaWarn).collect()
}

pub fn plan_notices(
    user: &TelemtUserLimits,
    now: DateTime<Utc>,
    config: &UserNotificationsConfig,
) -> Vec<PlannedNotice> {
    let Some(tg_user_id) = tg_id_from_telemt_username(&user.username) else {
        return Vec::new();
    };
    let limits = AccessLimits::new(
        user.expiration_rfc3339.as_deref(),
        user.data_quota_bytes,
        user.total_octets,
    );
    let mut notices = Vec::new();

    if let (Some(raw), Some(expires_at)) = (user.expiration_rfc3339.as_deref(), limits.expires_at) {
        let remaining = (expires_at - now).num_seconds();
        let planned = if remaining <= 0 {
            (-remaining <= EXPIRED_NOTIFY_WINDOW_SECS).then(|| (NoticeKind::Expired, Vec::new()))
        } else {
            let mut crossed: Vec<u32> = config
                .expiry_warn_days
                .iter()
                .copied()
                .filter(|days| remaining <= i64::from(*days) * SECS_PER_DAY)
                .collect();
            crossed.sort_unstable();
            crossed.dedup();
            crossed.split_first().map(|(nearest, rest)| {
                (
                    NoticeKind::ExpiryWarn(*nearest),
                    rest.iter().map(|d| NoticeKind::ExpiryWarn(*d)).collect(),
                )
            })
        };
        if let Some((kind, also_mark)) = planned {
            notices.push(PlannedNotice {
                tg_user_id,
                kind,
                also_mark,
                scope: raw.to_string(),
                limits: limits.clone(),
            });
        }
    }

    if let Some(quota) = limits.quota_bytes {
        let used_percent = u128::from(limits.used_bytes) * 100 / u128::from(quota);
        let mut crossed: Vec<u8> = config
            .quota_warn_percent
            .iter()
            .copied()
            .filter(|percent| used_percent >= u128::from(*percent))
            .collect();
        crossed.sort_unstable_by(|a, b| b.cmp(a));
        crossed.dedup();
        if let Some((highest, rest)) = crossed.split_first() {
            notices.push(PlannedNotice {
                tg_user_id,
                kind: NoticeKind::QuotaWarn(*highest),
                also_mark: rest.iter().map(|p| NoticeKind::QuotaWarn(*p)).collect(),
                scope: quota.to_string(),
                limits,
            });
        }
    }

    notices
}

pub fn spawn(bot: Bot, state: BotState) {
    let config = &state.config.user_notifications;
    if !config.enabled || !state.config.telemt_api.enabled {
        tracing::info!("User expiry and quota notifications are disabled");
        return;
    }
    let interval_secs = config.check_interval_secs;

    tokio::spawn(async move {
        tracing::info!(
            interval_secs = interval_secs,
            "Starting user expiry and quota checks"
        );
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = run_once(&bot, &state).await {
                tracing::warn!(error = %error, "Failed to check user expiry and quota");
            }
        }
    });
}

/// Ошибка одного пользователя не прерывает проход: она логируется, остальные
/// пользователи получают уведомления в этот же тик.
async fn run_once(bot: &Bot, state: &BotState) -> Result<(), anyhow::Error> {
    let Some(users) = state.telemt_backend.user_limits().await? else {
        return Ok(());
    };
    let now = Utc::now();
    let config = &state.config.user_notifications;
    for user in &users {
        if let Err(error) = forget_uncrossed_quota(state, user, config).await {
            tracing::warn!(
                username = %user.username,
                error = %error,
                "Failed to reset quota notification marks"
            );
        }
        for notice in plan_notices(user, now, config) {
            if let Err(error) = deliver(bot, state, &notice, now).await {
                tracing::warn!(
                    tg_user_id = notice.tg_user_id,
                    kind = %notice.kind.key(),
                    error = %error,
                    "Failed to process user notification"
                );
            }
        }
    }
    Ok(())
}

async fn forget_uncrossed_quota(
    state: &BotState,
    user: &TelemtUserLimits,
    config: &UserNotificationsConfig,
) -> Result<(), anyhow::Error> {
    let Some(tg_user_id) = tg_id_from_telemt_username(&user.username) else {
        return Ok(());
    };
    let kinds: Vec<String> = uncrossed_quota_notices(user, config)
        .into_iter()
        .map(NoticeKind::key)
        .collect();
    if kinds.is_empty() {
        return Ok(());
    }
    let removed = state
        .db
        .forget_user_notifications(tg_user_id, &kinds, None)
        .await?;
    if removed > 0 {
        tracing::info!(
            tg_user_id = tg_user_id,
            removed = removed,
            "Traffic dropped below quota threshold; warning re-armed"
        );
    }
    Ok(())
}

/// Ошибки, после которых повторять отправку бессмысленно: Telegram отклонил сообщение
/// (бот заблокирован, чат не найден). Сетевые ошибки и 429 — временные.
fn is_permanent_send_error(error: &RequestError) -> bool {
    matches!(error, RequestError::Api(_))
}

/// Отметка ставится до отправки, чтобы два прохода не отправили одно уведомление дважды.
/// При временной ошибке доставки она снимается, и следующий проход повторит отправку.
async fn deliver(
    bot: &Bot,
    state: &BotState,
    notice: &PlannedNotice,
    now: DateTime<Utc>,
) -> Result<(), anyhow::Error> {
    let db = &state.db;
    if !db
        .claim_user_notification(notice.tg_user_id, &notice.kind.key(), &notice.scope)
        .await?
    {
        return Ok(());
    }
    let mut claimed = vec![notice.kind.key()];
    for kind in &notice.also_mark {
        if db
            .claim_user_notification(notice.tg_user_id, &kind.key(), &notice.scope)
            .await?
        {
            claimed.push(kind.key());
        }
    }

    let vars = notice.limits.template_vars(now);
    let messages = &state.config.bot_messages;
    let text = match notice.kind {
        NoticeKind::ExpiryWarn(_) => messages.access_expiring_text(&vars),
        NoticeKind::Expired => messages.access_expired_text(&vars),
        NoticeKind::QuotaWarn(_) => messages.quota_warning_text(&vars),
    };
    let result = bot.send_message(ChatId(notice.tg_user_id), text).await;
    tokio::time::sleep(SEND_PAUSE).await;
    match result {
        Ok(_) => tracing::info!(
            tg_user_id = notice.tg_user_id,
            kind = %notice.kind.key(),
            scope = %notice.scope,
            "User notified about expiry or quota"
        ),
        Err(error) if is_permanent_send_error(&error) => tracing::warn!(
            tg_user_id = notice.tg_user_id,
            kind = %notice.kind.key(),
            error = %error,
            "Telegram rejected expiry or quota notification; not retrying"
        ),
        Err(error) => {
            tracing::warn!(
                tg_user_id = notice.tg_user_id,
                kind = %notice.kind.key(),
                error = %error,
                "Failed to send expiry or quota notification; will retry next check"
            );
            db.forget_user_notifications(notice.tg_user_id, &claimed, Some(&notice.scope))
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AccessLimits, NoticeKind, PlannedNotice, is_permanent_send_error, plan_notices,
        uncrossed_quota_notices,
    };
    use crate::config::UserNotificationsConfig;
    use crate::telemt_backend::TelemtUserLimits;
    use chrono::{DateTime, Utc};

    const GB: u64 = 1024 * 1024 * 1024;

    fn now() -> DateTime<Utc> {
        "2026-10-30T12:00:00Z".parse().unwrap()
    }

    fn user(expiration: Option<&str>, quota: Option<u64>, used: u64) -> TelemtUserLimits {
        TelemtUserLimits {
            username: "tg_7".to_string(),
            expiration_rfc3339: expiration.map(str::to_string),
            data_quota_bytes: quota,
            total_octets: used,
        }
    }

    fn kinds(notices: &[PlannedNotice]) -> Vec<(NoticeKind, Vec<NoticeKind>)> {
        notices
            .iter()
            .map(|n| (n.kind, n.also_mark.clone()))
            .collect()
    }

    #[test]
    fn no_limits_means_no_notices() {
        let config = UserNotificationsConfig::default();
        assert!(plan_notices(&user(None, None, 5 * GB), now(), &config).is_empty());
        assert!(plan_notices(&user(None, Some(0), 5 * GB), now(), &config).is_empty());
    }

    #[test]
    fn non_tg_users_are_skipped() {
        let mut u = user(Some("2026-10-31T00:00:00Z"), None, 0);
        u.username = "web_pilot".to_string();
        assert!(plan_notices(&u, now(), &UserNotificationsConfig::default()).is_empty());
    }

    #[test]
    fn expiry_thresholds() {
        let config = UserNotificationsConfig::default();
        let plan = |exp: &str| kinds(&plan_notices(&user(Some(exp), None, 0), now(), &config));

        // Больше 3 дней — рано.
        assert!(plan("2026-11-03T12:00:01Z").is_empty());
        // Ровно 3 дня и меньше — порог 3.
        assert_eq!(
            plan("2026-11-02T12:00:00Z"),
            vec![(NoticeKind::ExpiryWarn(3), vec![])]
        );
        // Меньше суток — одно сообщение по порогу 1, порог 3 только отмечается.
        assert_eq!(
            plan("2026-10-31T06:00:00Z"),
            vec![(NoticeKind::ExpiryWarn(1), vec![NoticeKind::ExpiryWarn(3)])]
        );
        // Истёк недавно.
        assert_eq!(
            plan("2026-10-30T11:59:00Z"),
            vec![(NoticeKind::Expired, vec![])]
        );
        // Истёк больше 7 дней назад — молчим.
        assert!(plan("2026-10-20T00:00:00Z").is_empty());
        // Некорректная дата — молчим.
        assert!(plan("not a date").is_empty());
    }

    #[test]
    fn expiry_scope_is_raw_expiration() {
        let notices = plan_notices(
            &user(Some("2026-10-31T15:00:00+03:00"), None, 0),
            now(),
            &UserNotificationsConfig::default(),
        );
        assert_eq!(notices[0].scope, "2026-10-31T15:00:00+03:00");
    }

    #[test]
    fn quota_thresholds() {
        let config = UserNotificationsConfig {
            quota_warn_percent: vec![80, 95],
            ..UserNotificationsConfig::default()
        };
        let plan = |used: u64| {
            kinds(&plan_notices(
                &user(None, Some(100 * GB), used),
                now(),
                &config,
            ))
        };

        assert!(plan(79 * GB).is_empty());
        assert_eq!(plan(80 * GB), vec![(NoticeKind::QuotaWarn(80), vec![])]);
        assert_eq!(
            plan(120 * GB),
            vec![(NoticeKind::QuotaWarn(95), vec![NoticeKind::QuotaWarn(80)])]
        );

        let notices = plan_notices(&user(None, Some(100 * GB), 90 * GB), now(), &config);
        assert_eq!(notices[0].scope, (100 * GB).to_string());
    }

    #[test]
    fn quota_marks_below_usage_are_reset() {
        let config = UserNotificationsConfig {
            quota_warn_percent: vec![80, 95],
            ..UserNotificationsConfig::default()
        };
        let uncrossed =
            |used: u64| uncrossed_quota_notices(&user(None, Some(100 * GB), used), &config);

        // Счётчик сброшен — оба порога снова ждут превышения.
        assert_eq!(
            uncrossed(0),
            vec![NoticeKind::QuotaWarn(80), NoticeKind::QuotaWarn(95)]
        );
        assert_eq!(uncrossed(90 * GB), vec![NoticeKind::QuotaWarn(95)]);
        assert!(uncrossed(95 * GB).is_empty());
        // Без квоты снимать нечего.
        assert!(uncrossed_quota_notices(&user(None, None, 0), &config).is_empty());
    }

    #[test]
    fn only_telegram_rejections_are_permanent() {
        use teloxide::{ApiError, RequestError};
        assert!(is_permanent_send_error(&RequestError::Api(
            ApiError::BotBlocked
        )));
        assert!(!is_permanent_send_error(&RequestError::RetryAfter(
            teloxide::types::Seconds::from_seconds(5)
        )));
    }

    #[test]
    fn empty_quota_thresholds_disable_quota_notices() {
        let config = UserNotificationsConfig {
            quota_warn_percent: Vec::new(),
            ..UserNotificationsConfig::default()
        };
        assert!(plan_notices(&user(None, Some(GB), 2 * GB), now(), &config).is_empty());
    }

    #[test]
    fn days_left_rounds_up() {
        let limits = |exp: &str| AccessLimits::new(Some(exp), None, 0);
        assert_eq!(limits("2026-10-30T13:00:00Z").days_left(now()), Some(1));
        assert_eq!(limits("2026-11-01T12:00:00Z").days_left(now()), Some(2));
        assert_eq!(limits("2026-11-01T12:00:01Z").days_left(now()), Some(3));
        assert_eq!(limits("2026-10-29T00:00:00Z").days_left(now()), Some(0));
        assert_eq!(AccessLimits::new(None, None, 0).days_left(now()), None);
    }

    #[test]
    fn status_lines_show_only_configured_limits() {
        assert_eq!(AccessLimits::new(None, None, GB).status_lines(now()), None);
        assert_eq!(
            AccessLimits::new(Some("2026-11-04T09:30:00Z"), Some(20 * GB), 5 * GB)
                .status_lines(now())
                .as_deref(),
            Some(
                "⏳ Доступ до 04.11.2026 09:30 UTC (осталось дней: 5)\n📊 Трафик: 5.00 GB из 20.00 GB"
            )
        );
        assert_eq!(
            AccessLimits::new(Some("2026-10-29T00:00:00Z"), None, 0)
                .status_lines(now())
                .as_deref(),
            Some("⛔ Срок доступа истёк 29.10.2026 00:00 UTC")
        );
    }

    #[test]
    fn template_vars_fill_all_placeholders() {
        let vars =
            AccessLimits::new(Some("2026-11-01T12:00:00Z"), Some(10 * GB), GB).template_vars(now());
        assert_eq!(
            vars,
            vec![
                ("expires_at", "01.11.2026 12:00 UTC".to_string()),
                ("days_left", "2".to_string()),
                ("used", "1.00 GB".to_string()),
                ("quota", "10.00 GB".to_string()),
            ]
        );
    }
}
