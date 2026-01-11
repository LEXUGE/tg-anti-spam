use crate::state::AppState;
use std::sync::Arc;
use teloxide::types::{ChatId, UserId};

#[derive(Clone)]
pub struct Filter {
    state: Arc<AppState>,
    threshold: u64,
}

impl Filter {
    pub fn new(state: Arc<AppState>, threshold: u64) -> Self {
        Self { state, threshold }
    }

    pub fn should_process(&self, chat_id: ChatId, user_id: UserId) -> bool {
        let count = self.state.increment(chat_id, user_id);
        count <= self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_threshold() {
        let state = Arc::new(AppState::new());
        let pre = Filter::new(state.clone(), 2);
        let cid = ChatId(1);
        let uid = UserId(100);

        assert!(pre.should_process(cid, uid));
        assert!(pre.should_process(cid, uid));
        assert!(!pre.should_process(cid, uid));
    }
}
