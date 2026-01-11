use crate::detect::SpamCheckResult;
use teloxide::prelude::*;
use teloxide::types::{ChatPermissions, Message, ReplyParameters};
use tracing::info;

pub async fn process_spam(bot: &Bot, message: &Message, res: SpamCheckResult) {
    let user = message.from.as_ref();
    let user_info = match user {
        Some(u) => format!("id={}, username={:?}", u.id, u.username),
        None => "unknown".to_string(),
    };

    let chat = &message.chat;

    info!(
        "Chat: {} ({}) | User: {} | Type: {:?}",
        chat.title().unwrap_or(""),
        chat.id,
        user_info,
        res.msg_type,
    );

    // Reply to the message with the spam detection result
    let reply_text = format!(
        "🚫 Spam detected!\n\nType: {:?}\n\nUser has been restricted for 24 hours.",
        res.msg_type
    );

    if let Err(e) = bot
        .send_message(chat.id, reply_text)
        .reply_parameters(ReplyParameters::new(message.id))
        .await
    {
        tracing::error!("Failed to reply to spam message: {}", e);
    }

    // Ban the user for 1 day (24 hours)
    if let Some(user) = user {
        let until_date = chrono::Utc::now() + chrono::Duration::days(1);

        // Restrict user permissions (no sending messages) for 1 day
        if let Err(e) = bot
            .restrict_chat_member(chat.id, user.id, ChatPermissions::empty())
            .until_date(until_date)
            .await
        {
            tracing::error!("Failed to restrict user {}: {}", user.id, e);
        } else {
            info!("User {} restricted until {}", user.id, until_date);
        }
    }
}
