use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use teloxide::types::{ChatId, UserId};
use tokio::fs;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AppState {
    pub counters: DashMap<String, u64>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn load_from_file(path: &str) -> anyhow::Result<Self> {
        if let Ok(content) = fs::read_to_string(path).await {
            let state: AppState = serde_json::from_str(&content)?;
            Ok(state)
        } else {
            Ok(Self::new())
        }
    }

    pub async fn save_to_file(&self, path: &str) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).await?;
        Ok(())
    }

    pub fn key(chat_id: ChatId, user_id: UserId) -> String {
        format!("{}:{}", user_id, chat_id)
    }

    #[allow(dead_code)]
    pub fn get_count(&self, chat_id: ChatId, user_id: UserId) -> u64 {
        let key = Self::key(chat_id, user_id);
        self.counters.get(&key).map(|v| *v.value()).unwrap_or(0)
    }

    /// Increment and return the updated count
    pub fn increment(&self, chat_id: ChatId, user_id: UserId) -> u64 {
        let key = Self::key(chat_id, user_id);
        let mut entry = self.counters.entry(key).or_insert(0);
        *entry += 1;
        *entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use teloxide::types::{ChatId, UserId};

    #[test]
    fn test_increment_and_persistence() {
        let state = AppState::new();
        let cid = ChatId(1);
        let uid = UserId(100);

        assert_eq!(state.get_count(cid, uid), 0);
        assert_eq!(state.increment(cid, uid), 1);
        assert_eq!(state.get_count(cid, uid), 1);
        assert_eq!(state.increment(cid, uid), 2);
    }
}
