mod bot;
mod config;
mod detect;
mod post;
mod pre;
mod state;

use crate::config::Settings;
use crate::detect::Agent;
use crate::pre::Filter;
use crate::state::AppState;
use std::sync::Arc;
use teloxide::Bot;
use tokio::time::{self, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Setup logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // 2. Load Config
    let settings = Arc::new(Settings::new().expect("Failed to load settings"));

    // 3. Load State
    let state = match AppState::load_from_file(&settings.state_path).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to load state, starting fresh: {}", e);
            AppState::default()
        }
    };
    let state = Arc::new(state);

    // 4. Initialize Components
    let agent = Arc::new(Agent::new(settings.gemini_api_key.clone()));
    let pre = Arc::new(Filter::new(state.clone(), settings.check_threshold));

    // 5. Background Task: Periodic Save
    let state_for_save = state.clone();
    let state_path = settings.state_path.clone(); // Clone for 'static lifetime
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(e) = state_for_save.save_to_file(&state_path).await {
                tracing::error!("Failed to save state: {}", e);
            }
        }
    });

    // 6. Start Bot
    let bot = Bot::new(settings.tg_bot_token.clone());
    tracing::info!("Starting Anti-Spam Bot...");

    bot::run_bot(bot, agent, pre, state, settings).await?;

    Ok(())
}
