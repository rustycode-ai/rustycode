//! Local HTTP callback server for OAuth 2.0 authorization code flow.
//!
//! Starts a temporary TCP listener on localhost, captures the `?code=&state=`
//! callback from the browser, validates CSRF state, and returns the auth code.

use crate::{AuthError, AuthResult};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::time::{timeout, Duration};

/// Default callback port range to try.
const PORT_RANGE: [u16; 3] = [9090, 9091, 9092];

/// Timeout for waiting on the browser callback.
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Maximum bytes to read from the incoming HTTP request.
const MAX_REQUEST_SIZE: usize = 4096;

/// The captured OAuth callback result.
#[derive(Debug, Clone)]
pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

/// Local HTTP server that listens for OAuth callbacks on localhost.
pub struct CallbackServer {
    listener: TcpListener,
    addr: SocketAddr,
}

impl CallbackServer {
    /// Bind to the first available port in the default range.
    pub async fn bind() -> AuthResult<Self> {
        for &port in &PORT_RANGE {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            if let Ok(listener) = TcpListener::bind(addr).await {
                let addr = listener
                    .local_addr()
                    .map_err(|e| AuthError::OAuth(format!("failed to get local addr: {e}")))?;
                return Ok(Self { listener, addr });
            }
        }
        Err(AuthError::OAuth(
            "no available port in callback range 9090-9092".to_string(),
        ))
    }

    /// Bind to a specific port.
    pub async fn bind_port(port: u16) -> AuthResult<Self> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| AuthError::OAuth(format!("failed to bind port {port}: {e}")))?;
        let addr = listener
            .local_addr()
            .map_err(|e| AuthError::OAuth(format!("failed to get local addr: {e}")))?;
        Ok(Self { listener, addr })
    }

    /// The redirect URL to use in the OAuth authorize request.
    pub fn redirect_url(&self) -> String {
        format!("http://localhost:{}/callback", self.addr.port())
    }

    /// The port this server is listening on.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Wait for a single callback, validate state, and return the auth code.
    ///
    /// Times out after `timeout_secs` seconds (default 120s).
    pub async fn wait_for_callback(
        &self,
        expected_state: &str,
        timeout_secs: Option<u64>,
    ) -> AuthResult<CallbackResult> {
        let secs = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
        let result = timeout(Duration::from_secs(secs), self.accept_one(expected_state)).await;

        match result {
            Ok(Ok(callback)) => Ok(callback),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AuthError::OAuth(
                "timed out waiting for browser callback".to_string(),
            )),
        }
    }

    /// Accept one HTTP connection, parse the callback, validate state.
    async fn accept_one(&self, expected_state: &str) -> AuthResult<CallbackResult> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (stream, _) = self
            .listener
            .accept()
            .await
            .map_err(|e| AuthError::OAuth(format!("failed to accept connection: {e}")))?;

        let mut buf = vec![0u8; MAX_REQUEST_SIZE];
        let (mut read_half, mut write_half) = tokio::io::split(stream);
        let bytes_read = read_half
            .read(&mut buf)
            .await
            .map_err(|e| AuthError::OAuth(format!("failed to read callback: {e}")))?;

        let request = String::from_utf8_lossy(&buf[..bytes_read]);

        // Parse the first line: "GET /callback?code=X&state=Y HTTP/1.1"
        let first_line = request.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("");

        let params = parse_query_params(path);

        // Send HTML response to browser
        let (body, status) = if let Some(error) = params.get("error") {
            let desc = match params.get("error_description") {
                Some(d) => d.as_str(),
                None => error,
            };
            (
                format!(
                    "<html><body><h2>Authentication failed</h2><p>{desc}</p>\
                     <p>You can close this tab.</p></body></html>"
                ),
                "400 Bad Request",
            )
        } else {
            (
                "<html><body><h2>Authentication successful!</h2>\
                 <p>You can close this tab and return to RustyCode.</p></body></html>"
                    .to_string(),
                "200 OK",
            )
        };

        let response = format!(
            "HTTP/1.1 {status}\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        if let Err(e) = write_half.write_all(response.as_bytes()).await {
            tracing::warn!("Failed to send callback response to browser: {}", e);
        }
        let _ = write_half.shutdown().await;

        // Check for OAuth error
        if let Some(error) = params.get("error") {
            return Err(AuthError::OAuth(format!(
                "provider returned error: {error}"
            )));
        }

        let code = params
            .get("code")
            .ok_or_else(|| AuthError::OAuth("callback missing 'code' parameter".into()))?
            .clone();

        let state = params
            .get("state")
            .ok_or_else(|| AuthError::OAuth("callback missing 'state' parameter".into()))?
            .clone();

        // Validate CSRF state
        if state != expected_state {
            return Err(AuthError::OAuth(
                "CSRF state mismatch — possible forgery".into(),
            ));
        }

        Ok(CallbackResult { code, state })
    }
}

/// Parse query parameters from a URL path like `/callback?code=X&state=Y`.
fn parse_query_params(path: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();

    let query = path.split('?').nth(1).unwrap_or("");
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");
        if !key.is_empty() {
            // Decode URL-encoded value, and also convert '+' to space
            // (application/x-www-form-urlencoded uses + for spaces)
            let decoded_value = value.replace('+', " ");
            let decoded = match urlencoding::decode(&decoded_value) {
                Ok(s) => s.into_owned(),
                Err(_) => decoded_value,
            };
            params.insert(
                urlencoding::decode(key)
                    .unwrap_or_else(|_| key.into())
                    .to_string(),
                decoded,
            );
        }
    }

    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn parse_query_params_extracts_code_and_state() {
        let params = parse_query_params("/callback?code=abc123&state=xyz789");
        assert_eq!(params.get("code").unwrap(), "abc123");
        assert_eq!(params.get("state").unwrap(), "xyz789");
    }

    #[test]
    fn parse_query_params_handles_empty_query() {
        let params = parse_query_params("/callback");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_query_params_handles_error_response() {
        let params =
            parse_query_params("/callback?error=access_denied&error_description=User+cancelled");
        assert_eq!(params.get("error").unwrap(), "access_denied");
        assert_eq!(params.get("error_description").unwrap(), "User cancelled");
    }

    #[test]
    fn parse_query_params_handles_url_encoded_values() {
        let params = parse_query_params("/callback?code=abc%20def&state=xyz%2F123");
        assert_eq!(params.get("code").unwrap(), "abc def");
        assert_eq!(params.get("state").unwrap(), "xyz/123");
    }

    #[test]
    fn parse_query_params_ignores_empty_pairs() {
        let params = parse_query_params("/callback?code=abc&&state=xyz&");
        assert_eq!(params.get("code").unwrap(), "abc");
        assert_eq!(params.get("state").unwrap(), "xyz");
    }

    #[tokio::test]
    async fn callback_server_binds_to_available_port() {
        let server = CallbackServer::bind().await.expect("should bind");
        assert!((9090..=9092).contains(&server.port()));
    }

    #[tokio::test]
    async fn callback_server_redirect_url_matches_port() {
        let server = CallbackServer::bind().await.expect("should bind");
        let url = server.redirect_url();
        assert!(url.starts_with("http://localhost:"));
        assert!(url.ends_with("/callback"));
        assert!(url.contains(&server.port().to_string()));
    }

    #[tokio::test]
    async fn callback_server_bind_specific_port() {
        // Use a high port unlikely to be in use
        let server = CallbackServer::bind_port(59099).await.expect("should bind");
        assert_eq!(server.port(), 59099);
        assert_eq!(server.redirect_url(), "http://localhost:59099/callback");
    }

    #[tokio::test]
    async fn callback_server_wait_times_out() {
        let server = CallbackServer::bind_port(59091).await.expect("should bind");
        let result = server.wait_for_callback("test-state", Some(1)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn callback_server_accepts_valid_callback() {
        let server = CallbackServer::bind_port(59092).await.expect("should bind");
        let port = server.port();
        let expected_state = "test-csrf-state-123";

        // Spawn a task that simulates the browser callback
        let state = expected_state.to_string();
        let handle = tokio::spawn(async move {
            // Give the server a moment to start listening
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let url = format!("http://localhost:{port}/callback?code=my-auth-code&state={state}");
            // Use a simple TCP connection to send the HTTP request
            let stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
                .await
                .expect("should connect");
            let request = format!("GET {url} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n");

            let (_, mut write_half) = tokio::io::split(stream);
            write_half
                .write_all(request.as_bytes())
                .await
                .expect("should write");
            write_half.shutdown().await.ok();
        });

        let result = server
            .wait_for_callback(expected_state, Some(5))
            .await
            .expect("should get callback");

        assert_eq!(result.code, "my-auth-code");
        assert_eq!(result.state, expected_state);

        handle.await.expect("task should complete");
    }

    #[tokio::test]
    async fn callback_server_rejects_state_mismatch() {
        let server = CallbackServer::bind_port(59093).await.expect("should bind");
        let port = server.port();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let url = format!("http://localhost:{port}/callback?code=abc&state=wrong-state");
            let stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
                .await
                .expect("should connect");
            let request = format!("GET {url} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n");

            let (_, mut write_half) = tokio::io::split(stream);
            write_half.write_all(request.as_bytes()).await.ok();
            write_half.shutdown().await.ok();
        });

        let result = server.wait_for_callback("expected-state", Some(5)).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("CSRF state mismatch"));

        handle.await.expect("task should complete");
    }

    #[tokio::test]
    async fn callback_server_handles_provider_error() {
        let server = CallbackServer::bind_port(59094).await.expect("should bind");
        let port = server.port();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let url = format!(
                "http://localhost:{port}/callback?error=access_denied&error_description=User+denied"
            );
            let stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
                .await
                .expect("should connect");
            let request = format!("GET {url} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n");

            let (_, mut write_half) = tokio::io::split(stream);
            write_half.write_all(request.as_bytes()).await.ok();
            write_half.shutdown().await.ok();
        });

        let result = server.wait_for_callback("any-state", Some(5)).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("access_denied"));

        handle.await.expect("task should complete");
    }
}
