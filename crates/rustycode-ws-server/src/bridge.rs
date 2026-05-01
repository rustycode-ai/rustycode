use std::sync::Arc;

use tracing::info;

use crate::session::SessionManager;

pub struct EventBridge {
    pub(crate) session_manager: Arc<SessionManager>,
    pub(crate) session_token: String,
    pub(crate) task_id: String,
}

impl EventBridge {
    pub const fn new(
        session_manager: Arc<SessionManager>,
        session_token: String,
        task_id: String,
    ) -> Self {
        Self {
            session_manager,
            session_token,
            task_id,
        }
    }
}

impl Drop for EventBridge {
    fn drop(&mut self) {
        info!(
            session_id = %self.session_token,
            "event bridge dropped"
        );
    }
}
