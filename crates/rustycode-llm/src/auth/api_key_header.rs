//! API key header authentication (x-api-key, api-key, etc).

use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use secrecy::{ExposeSecret, SecretString};

use super::AuthMethod;

#[derive(Clone)]
pub struct ApiKeyHeaderAuth {
    header_name: String,
    api_key: SecretString,
}

impl ApiKeyHeaderAuth {
    pub fn new(header_name: impl Into<String>, api_key: SecretString) -> Self {
        Self {
            header_name: header_name.into(),
            api_key,
        }
    }
}

#[async_trait]
impl AuthMethod for ApiKeyHeaderAuth {
    async fn apply(&self, headers: &mut HeaderMap) -> Result<()> {
        let name = HeaderName::from_bytes(self.header_name.as_bytes())?;
        let val = HeaderValue::from_str(self.api_key.expose_secret())?;
        headers.insert(name, val);
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn AuthMethod> {
        Box::new(self.clone())
    }
}
