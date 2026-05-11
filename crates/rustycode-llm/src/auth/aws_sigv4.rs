//! AWS Sigv4 authentication for AWS-compatible Bedrock requests.

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};

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

    fn default_host(&self) -> String {
        format!("{}.{}.amazonaws.com", self.service, self.region)
    }

    fn amz_date(headers: &HeaderMap) -> String {
        headers
            .get("x-amz-date")
            .and_then(|value| value.to_str().ok())
            .map(std::borrow::ToOwned::to_owned)
            .unwrap_or_else(|| Utc::now().format("%Y%m%dT%H%M%SZ").to_string())
    }

    fn payload_hash(headers: &HeaderMap) -> String {
        headers
            .get("x-amz-content-sha256")
            .and_then(|value| value.to_str().ok())
            .map(std::borrow::ToOwned::to_owned)
            .unwrap_or_else(|| Self::sha256_hex(b""))
    }

    fn canonical_headers(headers: &HeaderMap) -> (String, String) {
        let mut entries = Vec::new();

        for (name, value) in headers {
            if name == AUTHORIZATION {
                continue;
            }

            let name = name.as_str().to_ascii_lowercase();
            let value = value
                .to_str()
                .unwrap_or("")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            entries.push((name, value));
        }

        entries.sort_by(|(left_name, left_value), (right_name, right_value)| {
            left_name
                .cmp(right_name)
                .then_with(|| left_value.cmp(right_value))
        });

        let signed_headers = entries
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(";");
        let mut canonical_headers = String::new();
        for (name, value) in entries {
            canonical_headers.push_str(&name);
            canonical_headers.push(':');
            canonical_headers.push_str(&value);
            canonical_headers.push('\n');
        }

        (canonical_headers, signed_headers)
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        Self::hex_encode(&digest)
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
        const BLOCK_SIZE: usize = 64;

        let mut key_block = [0_u8; BLOCK_SIZE];
        if key.len() > BLOCK_SIZE {
            key_block.copy_from_slice(&Sha256::digest(key));
        } else {
            key_block[..key.len()].copy_from_slice(key);
        }

        let mut inner = [0_u8; BLOCK_SIZE];
        let mut outer = [0_u8; BLOCK_SIZE];
        for index in 0..BLOCK_SIZE {
            inner[index] = key_block[index] ^ 0x36;
            outer[index] = key_block[index] ^ 0x5c;
        }

        let mut hasher = Sha256::new();
        hasher.update(inner);
        hasher.update(data);
        let inner_digest = hasher.finalize();

        let mut hasher = Sha256::new();
        hasher.update(outer);
        hasher.update(inner_digest);
        hasher.finalize().into()
    }

    fn signature_key(&self, date: &str) -> [u8; 32] {
        let date_key = Self::hmac_sha256(
            format!("AWS4{}", self.secret_key.expose_secret()).as_bytes(),
            date.as_bytes(),
        );
        let region_key = Self::hmac_sha256(&date_key, self.region.as_bytes());
        let service_key = Self::hmac_sha256(&region_key, self.service.as_bytes());
        Self::hmac_sha256(&service_key, b"aws4_request")
    }

    fn hex_encode(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }
}

#[async_trait]
impl AuthMethod for AwsSigv4Auth {
    async fn apply(&self, headers: &mut HeaderMap) -> Result<()> {
        let host = headers
            .get("host")
            .and_then(|value| value.to_str().ok())
            .map(std::borrow::ToOwned::to_owned)
            .unwrap_or_else(|| self.default_host());
        let amz_date = Self::amz_date(headers);
        let payload_hash = Self::payload_hash(headers);

        headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_str(&host)?,
        );
        headers.insert(
            HeaderName::from_static("x-amz-date"),
            HeaderValue::from_str(&amz_date)?,
        );
        headers.insert(
            HeaderName::from_static("x-amz-content-sha256"),
            HeaderValue::from_str(&payload_hash)?,
        );

        let (canonical_headers, signed_headers) = Self::canonical_headers(headers);
        let canonical_request =
            format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");
        let canonical_request_hash = Self::sha256_hex(canonical_request.as_bytes());
        let date = amz_date.chars().take(8).collect::<String>();
        let credential_scope = format!("{date}/{}/{}/aws4_request", self.region, self.service);
        let string_to_sign =
            format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{canonical_request_hash}");
        let signature = Self::hex_encode(&Self::hmac_sha256(
            &self.signature_key(&date),
            string_to_sign.as_bytes(),
        ));

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key.expose_secret()
        );
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&authorization)?);
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn AuthMethod> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> AwsSigv4Auth {
        AwsSigv4Auth::new(
            SecretString::new("AKIDEXAMPLE".into()),
            SecretString::new("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into()),
            "us-east-1".to_string(),
            "bedrock-runtime".to_string(),
        )
    }

    #[tokio::test]
    async fn apply_adds_sigv4_headers() {
        let auth = auth();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-amz-date"),
            HeaderValue::from_static("20240511T120000Z"),
        );

        auth.apply(&mut headers).await.expect("sigv4 headers");

        assert_eq!(
            headers.get("host").and_then(|v| v.to_str().ok()),
            Some("bedrock-runtime.us-east-1.amazonaws.com")
        );
        assert_eq!(
            headers.get("x-amz-date").and_then(|v| v.to_str().ok()),
            Some("20240511T120000Z")
        );
        assert_eq!(
            headers
                .get("x-amz-content-sha256")
                .and_then(|v| v.to_str().ok()),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );

        let authorization = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .expect("authorization header");
        assert!(authorization.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20240511/us-east-1/bedrock-runtime/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="
        ));

        let signature = authorization
            .split("Signature=")
            .nth(1)
            .expect("signature suffix");
        assert_eq!(signature.len(), 64);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn apply_preserves_existing_host_and_signs_extra_headers() {
        let auth = auth();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_static("example.internal"),
        );
        headers.insert(
            HeaderName::from_static("x-amz-date"),
            HeaderValue::from_static("20240511T120000Z"),
        );
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );

        auth.apply(&mut headers).await.expect("sigv4 headers");

        assert_eq!(
            headers.get("host").and_then(|v| v.to_str().ok()),
            Some("example.internal")
        );
        let authorization = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .expect("authorization header");
        assert!(authorization
            .contains("SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date"));
    }
}
