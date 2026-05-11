//! Bearer token authentication.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use secrecy::{ExposeSecret, SecretString};

use super::AuthMethod;

#[derive(Clone)]
pub struct BearerAuth {
    token: SecretString,
}

impl BearerAuth {
    pub fn new(token: SecretString) -> Self {
        Self { token }
    }
}

#[async_trait]
impl AuthMethod for BearerAuth {
    async fn apply(&self, headers: &mut HeaderMap) -> Result<()> {
        let val = format!("Bearer {}", self.token.expose_secret());
        let header_val = HeaderValue::from_str(&val)?;
        headers.insert(AUTHORIZATION, header_val);
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn AuthMethod> {
        Box::new(self.clone())
    }
}
