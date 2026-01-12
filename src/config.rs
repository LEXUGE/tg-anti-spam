use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub tg_bot_token: String,
    pub gemini_api_key: String,
    #[serde(default = "default_threshold")]
    pub check_threshold: u64,
    #[serde(default = "default_state_path")]
    pub state_path: String,
    #[serde(default = "default_context_messages")]
    pub context_messages: usize,
}

fn default_state_path() -> String {
    "state.json".to_string()
}

fn default_threshold() -> u64 {
    20
}

fn default_context_messages() -> usize {
    5
}

impl Settings {
    pub fn new() -> anyhow::Result<Self> {
        let s = Config::builder()
            .add_source(File::with_name("settings").required(false))
            .add_source(config::Environment::with_prefix("ANTISPAM").separator("__"))
            .build()?;

        s.try_deserialize().map_err(Into::into)
    }
}
