//! No authentication (for local or open providers).

use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::HeaderMap;

use super::AuthMethod;

#[derive(Clone)]
pub struct NoAuth;

#[async_trait]
impl AuthMethod for NoAuth {
    async fn apply(&self, _headers: &mut HeaderMap) -> Result<()> {
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn AuthMethod> {
        Box::new(self.clone())
    }
}
