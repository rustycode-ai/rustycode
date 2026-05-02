pub mod approval;
pub mod auth;
pub mod bridge;
pub mod error;
pub mod protocol;
pub mod router;
pub mod session;

pub use auth::AuthConfig;
pub use bridge::EventBridge;
pub use error::WsError;
pub use protocol::{ClientMessage, Envelope, ServerMessage};
pub use router::WsRouter;
pub use session::{
    SessionManager, ProviderInfo, ProviderListResponse, SwitchProviderRequest,
    SessionInfo, SkillInfo, SkillListResponse, SkillExecuteRequest,
    McpServerInfo, McpServerListResponse, McpAddServerRequest,
    McpServerConfig,
};
