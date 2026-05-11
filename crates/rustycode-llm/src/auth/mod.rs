//! LLM authentication methods.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::HeaderMap;

/// Generic trait for applying authentication to a request.
#[async_trait]
pub trait AuthMethod: Send + Sync {
    /// Apply authentication to the given headers.
    async fn apply(&self, headers: &mut HeaderMap) -> Result<()>;

    /// Clone this auth method into a box.
    fn clone_box(&self) -> Box<dyn AuthMethod>;
}

pub mod api_key_header;
pub mod aws_sigv4;
pub mod bearer;
pub mod none;
pub mod resolver;

pub use api_key_header::ApiKeyHeaderAuth;
pub use aws_sigv4::AwsSigv4Auth;
pub use bearer::BearerAuth;
pub use none::NoAuth;
pub use resolver::AuthResolver;
