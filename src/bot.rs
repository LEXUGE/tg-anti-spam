use crate::config::Settings;
use crate::detect::MsgType;
use crate::state::AppState;
use crate::{detect::Agent, post, pre::Filter};
use std::sync::Arc;
use teloxide::prelude::*;
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
    #[command(description = "Reset your message count")]
    Reset,
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
    let user = match msg.from.as_ref() {
        Some(u) => u,
        None => return Ok(()),
    };

    let chat_id = msg.chat.id;
    let user_id = user.id;

    match cmd {
        Command::Start => {
            bot.send_message(chat_id, "Hello! I am the Anti-Spam Bot.")
                .await?;
        }
        Command::Stats => {
            let count = state.get_count(chat_id, user_id);
            bot.send_message(chat_id, format!("Your message count: {}", count))
                .await?;
        }
        Command::Save => {
            if let Err(e) = state.save_to_file(&settings.state_path).await {
                bot.send_message(chat_id, format!("Failed to save state: {}", e))
                    .await?;
            } else {
                bot.send_message(chat_id, "State saved successfully.")
                    .await?;
            }
        }
        Command::Reset => {
            state.reset(chat_id, user_id);
            bot.send_message(chat_id, "Your message count has been reset to 0.")
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
                    post::process_spam(&bot, &msg, res).await;
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
