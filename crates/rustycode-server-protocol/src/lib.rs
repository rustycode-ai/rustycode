pub mod envelope;
pub mod requests;
pub mod responses;
pub mod notifications;

pub use envelope::{
    JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestId,
};

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
