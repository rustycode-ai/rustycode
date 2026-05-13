pub mod envelope;
pub mod notifications;
pub mod requests;
pub mod responses;

pub use envelope::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestId};

#[derive(Debug, Clone)]
pub enum ClientMessage {
    Request(JsonRpcRequest),
    Notification(JsonRpcNotification),
}

#[derive(Debug, Clone)]
pub enum ServerMessage {
    Response(JsonRpcResponse),
    Notification(notifications::Notification),
}
