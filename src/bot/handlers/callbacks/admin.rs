mod feedback;
mod groups;
mod home;
mod pending;
mod service;
mod tokens;
mod users;

use crate::bot::handlers::callback_data::CallbackAction;
use crate::bot::handlers::state::BotState;
use teloxide::prelude::{Bot, CallbackQuery};

type AdminActionResult = Result<bool, Box<dyn std::error::Error + Send + Sync>>;

pub async fn handle_admin_action(
    bot: &Bot,
    q: &CallbackQuery,
    state: &BotState,
    action: CallbackAction,
) -> AdminActionResult {
    if home::handle(bot, q, state, action.clone()).await? {
        return Ok(true);
    }
    if feedback::handle(bot, q, state, action.clone()).await? {
        return Ok(true);
    }
    if groups::handle(bot, q, state, action.clone()).await? {
        return Ok(true);
    }
    if pending::handle(bot, q, state, action.clone()).await? {
        return Ok(true);
    }
    if users::handle(bot, q, state, action.clone()).await? {
        return Ok(true);
    }
    if tokens::handle(bot, q, state, action.clone()).await? {
        return Ok(true);
    }
    if service::handle(bot, q, state, action).await? {
        return Ok(true);
    }

    Ok(false)
}
