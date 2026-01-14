use crate::detect::SpamCheckResult;
use crate::state::AppState;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{
    ChatPermissions, InlineKeyboardButton, InlineKeyboardMarkup, Message, MessageId,
};
use tracing::info;

pub async fn process_spam(
    bot: &Bot,
    message: &Message,
    res: SpamCheckResult,
    state: Arc<AppState>,
) {
    let user = message.from.as_ref();
    let user_display = match user {
        Some(u) => {
            let name = format!("{} {}", u.first_name, u.last_name.as_deref().unwrap_or(""))
                .trim()
                .to_string();
            format!("{} ({})", name, u.id)
        }
        None => "Unknown".to_string(),
    };

    let chat = &message.chat;
    let message_text = message
        .text()
        .unwrap_or("<no text>")
        .chars()
        .take(50)
        .collect::<String>();

    info!(
        "Chat: {} ({}) | User: {} | Type: {:?}",
        chat.title().unwrap_or(""),
        chat.id,
        user_display,
        res.msg_type,
    );

    // Delete the spam message
    if let Err(e) = bot.delete_message(chat.id, message.id).await {
        tracing::error!("Failed to delete spam message: {}", e);
    } else {
        info!("Deleted spam message from {}", user_display);
    }

    if let Some(user) = user {
        // Check if there's an existing notification for this user and delete it
        if let Some(existing_msg_id) = state.get_spam_notification(chat.id, user.id)
            && let Err(e) = bot
                .delete_message(chat.id, MessageId(existing_msg_id))
                .await
        {
            tracing::error!("Failed to delete old spam notification: {}", e);
        }

        // Ban user for 24 hours
        let until_date = chrono::Utc::now() + chrono::Duration::days(1);

        if let Err(e) = bot
            .restrict_chat_member(chat.id, user.id, ChatPermissions::empty())
            .until_date(until_date)
            .await
        {
            tracing::error!("Failed to restrict user {}: {}", user.id, e);
        } else {
            info!("User {} restricted until {}", user.id, until_date);
        }

        let keyboard = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("Dismiss (TU Only)", format!("dismiss:{}", user.id)),
            InlineKeyboardButton::callback("Kick (Admin Only)", format!("kick:{}", user.id)),
        ]]);

        let notification_text = format!(
            "Spam detected!\n\nType: {:?}\nUser: {}\nMessage (first 50 chars): <tg-spoiler>{}</tg-spoiler>\n\nUser has been banned for 24 hours.",
            res.msg_type, user_display, message_text
        );

        match bot
            .send_message(chat.id, notification_text)
            .parse_mode(teloxide::types::ParseMode::Html)
            .reply_markup(keyboard)
            .await
        {
            Ok(sent_msg) => {
                // Track this notification
                state.track_spam_notification(chat.id, user.id, sent_msg.id.0);
            }
            Err(e) => {
                tracing::error!("Failed to send spam notification: {}", e);
            }
        }
    }
}
