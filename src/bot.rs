use crate::config::Settings;
use crate::detect::MsgType;
use crate::state::AppState;
use crate::{detect::Agent, post, pre::Filter};
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ReplyParameters;
use teloxide::utils::command::BotCommands;

#[derive(BotCommands, Clone, Debug)]
#[command(
    rename_rule = "snake_case",
    description = "Supported commands:",
    parse_with = "split"
)]
// NOTE: Explicitly set zero argument tuple together with parse_with = "split" to prevent user from
// spamming using command invocation.
enum Command {
    #[command(description = "Start the bot")]
    Start(),
    #[command(description = "Show statistics")]
    Stats(),
    #[command(description = "Save state")]
    Save(),
    #[command(description = "Reset your message count")]
    Reset(),
    #[command(description = "Clear context")]
    ClearContext(),
}

pub async fn run_bot(
    bot: Bot,
    agent: Arc<Agent>,
    pre: Arc<Filter>,
    state: Arc<AppState>,
    settings: Arc<Settings>,
) -> anyhow::Result<()> {
    let command_handler = Update::filter_message()
        .filter_command::<Command>()
        .endpoint(handle_command);

    let message_handler = Update::filter_message().endpoint(handle_spam_check);

    let callback_handler = Update::filter_callback_query().endpoint(handle_callback_query);

    let handler = dptree::entry()
        .branch(command_handler)
        .branch(callback_handler)
        .branch(message_handler);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![agent, pre, state, settings])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: Arc<AppState>,
    settings: Arc<Settings>,
) -> ResponseResult<()> {
    let user = match msg.from.as_ref() {
        Some(u) => u,
        None => return Ok(()),
    };

    let chat_id = msg.chat.id;
    let user_id = user.id;

    match cmd {
        Command::Start() => {
            bot.send_message(chat_id, "Hello! I am an Anti-Spam Bot.")
                .await?;
        }
        Command::Stats() => {
            let count = state.get_count(chat_id, user_id);
            bot.send_message(chat_id, format!("Your message count: {}", count))
                .reply_parameters(ReplyParameters::new(msg.id))
                .await?;
        }
        Command::Save() => {
            if let Err(e) = state.save_to_file(&settings.state_path).await {
                bot.send_message(chat_id, format!("Failed to save state: {}", e))
                    .reply_parameters(ReplyParameters::new(msg.id))
                    .await?;
            } else {
                bot.send_message(chat_id, "State saved successfully.")
                    .reply_parameters(ReplyParameters::new(msg.id))
                    .await?;
            }
        }
        Command::Reset() => {
            state.reset(chat_id, user_id);
            bot.send_message(chat_id, "Your message count has been reset to 0.")
                .reply_parameters(ReplyParameters::new(msg.id))
                .await?;
        }
        Command::ClearContext() => {
            state.clear_context(chat_id);
            bot.send_message(chat_id, "Message context has been cleared.")
                .reply_parameters(ReplyParameters::new(msg.id))
                .await?;
        }
    }
    Ok(())
}

async fn handle_spam_check(
    bot: Bot,
    msg: Message,
    agent: Arc<Agent>,
    pre: Arc<Filter>,
    state: Arc<AppState>,
    settings: Arc<Settings>,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    let user_id = match msg.from.as_ref() {
        Some(u) => u.id,
        None => return Ok(()),
    };

    if !pre.should_process(chat_id, user_id) {
        return Ok(());
    }

    if msg.text().is_some() {
        // Retrieve message history context
        let context = state.get_context(chat_id);

        match agent.check_spam(&msg, &context).await {
            Ok(res) => {
                if res.msg_type != MsgType::NotSpam {
                    post::process_spam(&bot, &msg, res, state.clone()).await;
                }
            }
            Err(e) => {
                tracing::error!("Failed to check spam: {}", e);
            }
        }

        // Store message in history after processing (regardless of spam result)
        state.add_message(chat_id, msg, settings.context_messages);
    }

    Ok(())
}

async fn handle_callback_query(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
    settings: Arc<Settings>,
) -> ResponseResult<()> {
    match handle_callback_inner(&bot, &q, &state, &settings).await {
        Ok(msg) => {
            bot.answer_callback_query(&q.id).text(msg).await?;
        }
        Err(e) => {
            tracing::error!("Callback error: {}", e);
            bot.answer_callback_query(&q.id)
                .text(&e)
                .show_alert(true)
                .await?;
        }
    }
    Ok(())
}

async fn handle_callback_inner(
    bot: &Bot,
    q: &CallbackQuery,
    state: &AppState,
    settings: &Settings,
) -> Result<&'static str, String> {
    let data = q.data.as_ref().ok_or("No callback data")?;
    let message = q.message.as_ref().ok_or("Message not found")?;
    let chat_id = message.chat().id;
    let clicker = q.from.id;

    let (action, user_id_str) = data.split_once(':').ok_or("Invalid callback data")?;
    let user_id_raw = user_id_str.parse::<u64>().map_err(|_| "Invalid user ID")?;
    let banned_user_id = UserId(user_id_raw);

    match action {
        "dismiss" => {
            handle_dismiss(
                bot,
                state,
                settings,
                chat_id,
                clicker,
                banned_user_id,
                message,
            )
            .await
        }
        "kick" => handle_kick(bot, state, chat_id, clicker, banned_user_id, message).await,
        _ => Err("Unknown action".to_string()),
    }
}

async fn handle_dismiss(
    bot: &Bot,
    state: &AppState,
    settings: &Settings,
    chat_id: ChatId,
    clicker: UserId,
    banned_user_id: UserId,
    message: &teloxide::types::MaybeInaccessibleMessage,
) -> Result<&'static str, String> {
    if !state.is_trusted_user(chat_id, clicker, settings.check_threshold) {
        return Err("You must be a trusted user to dismiss this action".to_string());
    }

    bot.restrict_chat_member(
        chat_id,
        banned_user_id,
        teloxide::types::ChatPermissions::all(),
    )
    .await
    .map_err(|_| "Failed to unban user".to_string())?;

    let _ = bot.delete_message(chat_id, message.id()).await;
    state.remove_spam_notification(chat_id, banned_user_id);

    tracing::info!(
        "User {} dismissed ban for user {} in chat {}",
        clicker,
        banned_user_id,
        chat_id
    );

    Ok("User has been unbanned")
}

async fn handle_kick(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    clicker: UserId,
    banned_user_id: UserId,
    message: &teloxide::types::MaybeInaccessibleMessage,
) -> Result<&'static str, String> {
    let admins = bot
        .get_chat_administrators(chat_id)
        .await
        .map_err(|_| "Failed to verify permissions".to_string())?;

    if !admins.iter().any(|admin| admin.user.id == clicker) {
        return Err("Only administrators can kick users".to_string());
    }

    bot.ban_chat_member(chat_id, banned_user_id)
        .await
        .map_err(|_| "Failed to kick user".to_string())?;

    let _ = bot.delete_message(chat_id, message.id()).await;
    state.remove_spam_notification(chat_id, banned_user_id);

    tracing::info!(
        "User {} kicked user {} from chat {}",
        clicker,
        banned_user_id,
        chat_id
    );

    Ok("User has been permanently kicked")
}
