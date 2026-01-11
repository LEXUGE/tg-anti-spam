use crate::config::Settings;
use crate::detect::MsgType;
use crate::state::AppState;
use crate::{detect::Agent, post, pre::Filter};
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::UserId;
use teloxide::utils::command::BotCommands;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Supported commands:")]
enum Command {
    #[command(description = "Start the bot")]
    Start,
    #[command(description = "Show statistics")]
    Stats,
    #[command(description = "Save state")]
    Save,
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

    let handler = dptree::entry()
        .branch(command_handler)
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
    match cmd {
        Command::Start => {
            bot.send_message(msg.chat.id, "Hello! I am the Anti-Spam Bot.")
                .await?;
        }
        Command::Stats => {
            let count = state.get_count(
                msg.chat.id,
                msg.from.as_ref().map(|u| u.id).unwrap_or(UserId(0)),
            );
            bot.send_message(msg.chat.id, format!("Your message count: {}", count))
                .await?;
        }
        Command::Save => {
            if let Err(e) = state.save_to_file(&settings.state_path).await {
                bot.send_message(msg.chat.id, format!("Failed to save state: {}", e))
                    .await?;
            } else {
                bot.send_message(msg.chat.id, "State saved successfully.")
                    .await?;
            }
        }
    }
    Ok(())
}

async fn handle_spam_check(
    bot: Bot,
    msg: Message,
    agent: Arc<Agent>,
    pre: Arc<Filter>,
) -> ResponseResult<()> {
    // 1. Filter Logic (Rate Limiting / Quota)
    let chat_id = msg.chat.id;
    let user_id = match msg.from.as_ref() {
        Some(u) => u.id,
        None => return Ok(()),
    };

    if !pre.should_process(chat_id, user_id) {
        return Ok(());
    }

    // 2. Spam Check
    if let Some(text) = msg.text() {
        match agent.check_spam(text).await {
            Ok(res) => {
                if res.msg_type != MsgType::NotSpam {
                    post::process_spam(&bot, &msg, res).await;
                }
            }
            Err(e) => {
                tracing::error!("Failed to check spam: {}", e);
            }
        }
    }

    Ok(())
}
