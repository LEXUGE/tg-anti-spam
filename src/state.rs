use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use teloxide::types::{ChatId, Message, UserId};
use tokio::fs;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AppState {
    pub counters: DashMap<String, u64>,
    #[serde(default)]
    pub message_history: DashMap<i64, VecDeque<Message>>,
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

    /// Reset the counter for a specific user in a chat
    pub fn reset(&self, chat_id: ChatId, user_id: UserId) {
        let key = Self::key(chat_id, user_id);
        self.counters.remove(&key);
    }

    /// Add a message to the chat's history, maintaining a maximum size
    pub fn add_message(&self, chat_id: ChatId, message: Message, max_size: usize) {
        let chat_key = chat_id.0;
        let mut entry = self.message_history.entry(chat_key).or_default();

        entry.push_back(message);

        // Remove oldest messages if we exceed the limit
        while entry.len() > max_size {
            entry.pop_front();
        }
    }

    /// Clear message context for a specific chat_id
    pub fn clear_context(&self, chat_id: ChatId) {
        let chat_key = chat_id.0;
        if let Some(mut q) = self.message_history.get_mut(&chat_key) {
            q.clear()
        }
    }

    /// Get the message history for a chat
    pub fn get_context(&self, chat_id: ChatId) -> Vec<Message> {
        let chat_key = chat_id.0;
        self.message_history
            .get(&chat_key)
            .map(|queue| queue.iter().cloned().collect())
            .unwrap_or_default()
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
