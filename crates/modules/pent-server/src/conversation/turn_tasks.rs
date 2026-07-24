//! In-process lifecycle control for background conversation turns.

use futures_util::future::AbortHandle;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
pub(crate) struct TurnTaskRegistry {
    tasks: Arc<Mutex<HashMap<String, AbortHandle>>>,
}

impl TurnTaskRegistry {
    pub(crate) fn register(&self, turn_id: String, abort_handle: AbortHandle) {
        self.tasks
            .lock()
            .expect("turn task registry mutex poisoned")
            .insert(turn_id, abort_handle);
    }

    pub(crate) fn abort(&self, turn_id: &str) -> bool {
        let abort_handle = self
            .tasks
            .lock()
            .expect("turn task registry mutex poisoned")
            .remove(turn_id);
        if let Some(abort_handle) = abort_handle {
            abort_handle.abort();
            true
        } else {
            false
        }
    }

    pub(crate) fn remove(&self, turn_id: &str) {
        self.tasks
            .lock()
            .expect("turn task registry mutex poisoned")
            .remove(turn_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::{Abortable, pending};

    #[tokio::test]
    async fn abort_stops_a_registered_turn() {
        let registry = TurnTaskRegistry::default();
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        registry.register("turn-1".into(), abort_handle);

        let task = tokio::spawn(Abortable::new(pending::<()>(), abort_registration));

        assert!(registry.abort("turn-1"));
        assert!(task.await.expect("task should join").is_err());
        assert!(!registry.abort("turn-1"));
    }
}
