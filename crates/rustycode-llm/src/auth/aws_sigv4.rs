//! AWS Sigv4 authentication (placeholder).

use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use secrecy::SecretString;

use super::AuthMethod;

#[derive(Clone)]
pub struct AwsSigv4Auth {
    #[allow(dead_code)]
    access_key: SecretString,
    #[allow(dead_code)]
    secret_key: SecretString,
    #[allow(dead_code)]
    region: String,
    #[allow(dead_code)]
    service: String,
}

impl AwsSigv4Auth {
    pub fn new(
        access_key: SecretString,
        secret_key: SecretString,
        region: String,
        service: String,
    ) -> Self {
        Self {
            access_key,
            secret_key,
            region,
            service,
        }
    }
}

#[async_trait]
impl AuthMethod for AwsSigv4Auth {
    async fn apply(&self, _headers: &mut HeaderMap) -> Result<()> {
        // TODO: Implement real Sigv4 signing logic.
        // This requires canonicalizing the request, hashing the payload,
        // and generating the Authorization header signature.
        tracing::warn!("AwsSigv4Auth is currently a placeholder.");
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn AuthMethod> {
        Box::new(self.clone())
    }
}
