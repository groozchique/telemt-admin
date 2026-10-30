use crate::bot::handlers::callback_data::UserLimitField;
use crate::config::Config;
use crate::db::Db;
use crate::runtime::TelemtRuntime;
use crate::telemt_backend::TelemtBackend;
use std::sync::Arc;
use teloxide::types::Message;

#[derive(Clone, Debug)]
pub enum WizardState {
    AwaitingInviteToken,
    /// Сообщение обратной связи для администраторов. `media_group_id` задан, когда первое
    /// сообщение альбома уже отправлено: остальные части того же альбома идут следом.
    UserFeedback {
        media_group_id: Option<String>,
    },
    /// Ответ администратора пользователю на обратную связь (альбом — как в `UserFeedback`).
    AdminFeedbackReply {
        tg_user_id: i64,
        media_group_id: Option<String>,
    },
    AdminDeleteAwaitingTarget,
    AdminFindUserAwaitingTarget {
        page: i64,
    },
    AdminSetUserLimitAwaitingValue {
        tg_user_id: i64,
        page: i64,
        field: UserLimitField,
    },
    AdminFindTokenAwaitingCode {
        page: i64,
    },
    AdminTokenCreateAwaitingParams {
        auto_approve: bool,
    },
    /// Ожидание текста рассылки всем approved-пользователям.
    AdminBroadcastAwaitingMessage,
    /// Название новой группы пользователей.
    AdminGroupAwaitingName,
    /// Новое значение общего срока действия группы.
    AdminGroupExpiryAwaitingValue {
        group_id: i64,
    },
    /// Telegram user id для импорта из telemt API.
    AdminImportAwaitingTgId,
}

impl WizardState {
    fn encode(&self) -> String {
        match self {
            Self::AwaitingInviteToken => "awaiting_invite_token".to_string(),
            Self::UserFeedback { media_group_id } => match media_group_id {
                Some(group) => format!("user_feedback:{group}"),
                None => "user_feedback".to_string(),
            },
            Self::AdminFeedbackReply {
                tg_user_id,
                media_group_id,
            } => match media_group_id {
                Some(group) => format!("admin_feedback_reply:{tg_user_id}:{group}"),
                None => format!("admin_feedback_reply:{tg_user_id}"),
            },
            Self::AdminDeleteAwaitingTarget => "admin_delete_awaiting_target".to_string(),
            Self::AdminFindUserAwaitingTarget { page } => {
                format!("admin_find_user:{}", (*page).max(1))
            }
            Self::AdminSetUserLimitAwaitingValue {
                tg_user_id,
                page,
                field,
            } => format!(
                "admin_set_user_limit:{}:{}:{}",
                field.as_str(),
                tg_user_id,
                (*page).max(1)
            ),
            Self::AdminFindTokenAwaitingCode { page } => {
                format!("admin_find_token:{}", (*page).max(1))
            }
            Self::AdminTokenCreateAwaitingParams { auto_approve } => {
                format!(
                    "admin_token_create:{}",
                    if *auto_approve { "auto" } else { "manual" }
                )
            }
            Self::AdminBroadcastAwaitingMessage => "admin_broadcast_awaiting".to_string(),
            Self::AdminGroupAwaitingName => "admin_group_awaiting_name".to_string(),
            Self::AdminGroupExpiryAwaitingValue { group_id } => {
                format!("admin_group_expiry:{group_id}")
            }
            Self::AdminImportAwaitingTgId => "admin_import_awaiting_tg_id".to_string(),
        }
    }

    fn decode(value: &str) -> Option<Self> {
        match value {
            "awaiting_invite_token" => Some(Self::AwaitingInviteToken),
            "user_feedback" => Some(Self::UserFeedback {
                media_group_id: None,
            }),
            "admin_delete_awaiting_target" => Some(Self::AdminDeleteAwaitingTarget),
            "admin_token_create:auto" => {
                Some(Self::AdminTokenCreateAwaitingParams { auto_approve: true })
            }
            "admin_token_create:manual" => Some(Self::AdminTokenCreateAwaitingParams {
                auto_approve: false,
            }),
            "admin_broadcast_awaiting" => Some(Self::AdminBroadcastAwaitingMessage),
            "admin_group_awaiting_name" => Some(Self::AdminGroupAwaitingName),
            "admin_import_awaiting_tg_id" => Some(Self::AdminImportAwaitingTgId),
            _ => {
                if let Some(group) = value.strip_prefix("user_feedback:") {
                    return Some(Self::UserFeedback {
                        media_group_id: Some(group.to_string()).filter(|g| !g.is_empty()),
                    });
                }
                if let Some(value) = value.strip_prefix("admin_feedback_reply:") {
                    let (tg_user_id, group) = match value.split_once(':') {
                        Some((id, group)) => {
                            (id, Some(group.to_string()).filter(|g| !g.is_empty()))
                        }
                        None => (value, None),
                    };
                    return Some(Self::AdminFeedbackReply {
                        tg_user_id: tg_user_id.parse::<i64>().ok()?,
                        media_group_id: group,
                    });
                }
                if let Some(value) = value.strip_prefix("admin_group_expiry:") {
                    return Some(Self::AdminGroupExpiryAwaitingValue {
                        group_id: value.parse::<i64>().ok()?,
                    });
                }
                if let Some(value) = value.strip_prefix("admin_find_user:") {
                    return Some(Self::AdminFindUserAwaitingTarget {
                        page: value.parse::<i64>().ok()?.max(1),
                    });
                }
                if let Some(value) = value.strip_prefix("admin_set_user_limit:") {
                    let mut parts = value.split(':');
                    let field = UserLimitField::parse(parts.next()?)?;
                    let tg_user_id = parts.next()?.parse::<i64>().ok()?;
                    let page = parts.next()?.parse::<i64>().ok()?.max(1);
                    return Some(Self::AdminSetUserLimitAwaitingValue {
                        tg_user_id,
                        page,
                        field,
                    });
                }
                if let Some(value) = value.strip_prefix("admin_find_token:") {
                    return Some(Self::AdminFindTokenAwaitingCode {
                        page: value.parse::<i64>().ok()?.max(1),
                    });
                }
                None
            }
        }
    }
}

#[derive(Clone)]
pub struct BotState {
    pub config: Arc<Config>,
    pub db: Arc<Db>,
    pub telemt_backend: TelemtBackend,
    pub telemt_runtime: TelemtRuntime,
    pub bot_username: Option<String>,
}

pub fn telemt_username(tg_user_id: i64) -> String {
    format!("tg_{}", tg_user_id)
}

pub fn sender_user_id(msg: &Message) -> Option<i64> {
    msg.from.as_ref().map(|user| user.id.0 as i64)
}

pub fn sender_display_name(msg: &Message) -> Option<String> {
    msg.from.as_ref().map(|user| {
        let mut full_name = user.first_name.clone();
        if let Some(last_name) = user.last_name.as_deref()
            && !last_name.trim().is_empty()
        {
            full_name.push(' ');
            full_name.push_str(last_name);
        }
        full_name
    })
}

pub fn is_admin_message(msg: &Message, state: &BotState) -> bool {
    sender_user_id(msg).is_some_and(|user_id| state.config.is_admin(user_id))
}

pub async fn wizard_state(
    state: &BotState,
    user_id: i64,
) -> Result<Option<WizardState>, anyhow::Error> {
    let Some(state_key) = state.db.get_wizard_state(user_id).await? else {
        return Ok(None);
    };
    let Some(decoded) = WizardState::decode(&state_key) else {
        tracing::warn!(
            user_id = user_id,
            state_key = %state_key,
            "Не удалось декодировать сохранённое wizard-состояние"
        );
        state.db.clear_wizard_state(user_id).await?;
        return Ok(None);
    };
    Ok(Some(decoded))
}

pub async fn set_wizard_state(
    state: &BotState,
    user_id: i64,
    wizard_state: WizardState,
) -> Result<(), anyhow::Error> {
    state
        .db
        .set_wizard_state(user_id, &wizard_state.encode())
        .await?;
    Ok(())
}

pub async fn clear_wizard_state(state: &BotState, user_id: i64) -> Result<(), anyhow::Error> {
    state.db.clear_wizard_state(user_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::WizardState;

    fn roundtrip(state: WizardState) -> Option<WizardState> {
        WizardState::decode(&state.encode())
    }

    #[test]
    fn feedback_states_roundtrip() {
        assert!(matches!(
            roundtrip(WizardState::UserFeedback {
                media_group_id: None
            }),
            Some(WizardState::UserFeedback {
                media_group_id: None
            })
        ));
        assert!(matches!(
            roundtrip(WizardState::UserFeedback {
                media_group_id: Some("13579".to_string())
            }),
            Some(WizardState::UserFeedback { media_group_id: Some(group) }) if group == "13579"
        ));
        assert!(matches!(
            roundtrip(WizardState::AdminFeedbackReply {
                tg_user_id: 42,
                media_group_id: None
            }),
            Some(WizardState::AdminFeedbackReply {
                tg_user_id: 42,
                media_group_id: None
            })
        ));
        assert!(matches!(
            roundtrip(WizardState::AdminFeedbackReply {
                tg_user_id: 42,
                media_group_id: Some("777".to_string())
            }),
            Some(WizardState::AdminFeedbackReply { tg_user_id: 42, media_group_id: Some(group) })
                if group == "777"
        ));
    }

    #[test]
    fn feedback_reply_state_rejects_bad_user_id() {
        assert!(WizardState::decode("admin_feedback_reply:abc").is_none());
        assert!(WizardState::decode("admin_feedback_reply:").is_none());
    }
}
