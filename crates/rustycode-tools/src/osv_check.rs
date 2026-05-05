//! OSV (Open Source Vulnerabilities) Package Malware Checker
//!
//! Queries the OSV API at `api.osv.dev` to check if npm/PyPI packages
//! are flagged with `MAL-*` advisories before allowing installation.
//!
//! Supports:
//! - npm packages (`@scope/pkg@1.2.3`, `pkg@1.0.0`)
//! - `PyPI` packages (`package==1.2.3`)
//! - Paginated responses from OSV API
//! - Configurable endpoint via `OSV_ENDPOINT` env var
//! - Fail-open on API errors (never blocks legitimate installs)
//!
//! Ported from goose's `agents/extension_malware_check.rs`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default OSV API query endpoint
const DEFAULT_OSV_ENDPOINT: &str = "https://api.osv.dev/v1/query";

/// Error type for OSV check operations
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OsvError {
    #[error("Blocked malicious package: {name}@{version} ({ecosystem}). Advisories: {advisories}")]
    MaliciousPackage {
        name: String,
        version: String,
        ecosystem: String,
        advisories: String,
    },

    #[error("OSV check failed: {0}")]
    CheckFailed(String),
}

/// OSV package vulnerability checker
pub struct OsvChecker {
    client: reqwest::Client,
    endpoint: String,
}

impl OsvChecker {
    /// Create a new OSV checker with the default endpoint.
    ///
    /// Honors the `OSV_ENDPOINT` environment variable if set.
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("rustycode-osv-check/1.0")
            .build()
            .map_err(|e| OsvError::CheckFailed(format!("HTTP client build failed: {e}")))?;

        let endpoint =
            std::env::var("OSV_ENDPOINT").unwrap_or_else(|_| DEFAULT_OSV_ENDPOINT.to_string());

        Ok(Self { client, endpoint })
    }

    /// Create a checker with a custom endpoint (useful for testing).
    pub fn with_endpoint(endpoint: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("rustycode-osv-check/1.0")
            .build()
            .map_err(|e| OsvError::CheckFailed(format!("HTTP client build failed: {e}")))?;

        Ok(Self { client, endpoint })
    }

    /// Check if a package has any MAL-* advisories.
    ///
    /// Returns `Ok(())` if clean, `Err(OsvError::MaliciousPackage)` if malicious.
    /// Fails open on network/parse errors (returns `Ok(())`).
    pub async fn deny_if_malicious(
        &self,
        name: &str,
        ecosystem: &str,
        version: Option<&str>,
    ) -> Result<(), OsvError> {
        tracing::debug!("OSV query: name={name}, ecosystem={ecosystem:?}, version={version:?}");

        let mut page_token: Option<String> = None;
        let mut malicious: Vec<Vuln> = Vec::new();

        loop {
            let body = QueryRequest {
                version,
                package: PackageInfo {
                    name,
                    ecosystem,
                    purl: None,
                },
                page_token: page_token.clone(),
            };

            let resp = match self.client.post(&self.endpoint).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("OSV request failed for {ecosystem} {name}: {e}; failing open");
                    return Ok(());
                }
            };

            let payload: QueryResponse = match resp.json().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("OSV parse error for {ecosystem} {name}: {e}; failing open");
                    return Ok(());
                }
            };

            malicious.extend(
                payload
                    .vulns
                    .into_iter()
                    .filter(|v| v.id.starts_with("MAL-")),
            );

            match payload.next_page_token {
                Some(tok) if !tok.is_empty() => page_token = Some(tok),
                _ => break,
            }
        }

        if !malicious.is_empty() {
            let ver = version.unwrap_or("<any>");
            let details = malicious
                .into_iter()
                .map(|v| {
                    if v.summary.is_empty() {
                        v.id
                    } else {
                        format!("{} — {}", v.id, v.summary)
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");

            tracing::error!("Blocked malicious package: {name}@{ver} ({ecosystem}) — {details}");

            return Err(OsvError::MaliciousPackage {
                name: name.to_string(),
                version: ver.to_string(),
                ecosystem: ecosystem.to_string(),
                advisories: details,
            });
        }

        tracing::debug!("OSV: no MAL advisories for {name} ({ecosystem})");
        Ok(())
    }
}

impl Default for OsvChecker {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            tracing::warn!("OsvChecker::new() failed, using fallback client: {e}");
            Self {
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new()),
                endpoint: DEFAULT_OSV_ENDPOINT.to_string(),
            }
        })
    }
}

/// Convenience function: infer ecosystem from command name and check args.
///
/// - `npx` → npm ecosystem
/// - `uvx` / `pip` / `pipx` → `PyPI` ecosystem
/// - unknown commands → skip (fail open)
pub async fn deny_if_malicious_cmd_args(cmd: &str, args: &[String]) -> Result<()> {
    let ecosystem = if cmd.ends_with("uvx") || cmd.ends_with("pip") || cmd.ends_with("pipx") {
        "PyPI"
    } else if cmd.ends_with("npx") || cmd.ends_with("npm") {
        "npm"
    } else {
        tracing::debug!("Unknown ecosystem for command '{cmd}'; skipping OSV check (fail open)");
        return Ok(());
    };

    if let Some((name, version)) = parse_first_package_arg(ecosystem, args) {
        let checker =
            OsvChecker::new().map_err(|e| anyhow::anyhow!("OSV checker init failed: {e}"))?;
        checker
            .deny_if_malicious(&name, ecosystem, version.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    } else {
        tracing::debug!("No package token found for '{cmd}'; skipping OSV check");
    }

    Ok(())
}

// ── Package Argument Parsing ───────────────────────────────────────────────

/// Parse the first non-flag argument into (name, `optional_version`).
fn parse_first_package_arg(ecosystem: &str, args: &[String]) -> Option<(String, Option<String>)> {
    let is_flag = |s: &str| s.starts_with('-');
    let token = args
        .iter()
        .find(|a| !is_flag(a.as_str()))?
        .trim()
        .to_string();
    if token.is_empty() {
        return None;
    }
    match ecosystem {
        "npm" => parse_npm_token(&token),
        "PyPI" => parse_pypi_token(&token),
        _ => None,
    }
}

/// Parse npm package tokens: `@scope/pkg@1.2.3`, `react@18.3.1`, `eslint`
fn parse_npm_token(token: &str) -> Option<(String, Option<String>)> {
    if token.starts_with('@') {
        // Scoped package: @scope/pkg@1.2.3
        if let Some(idx) = token.rfind('@') {
            if idx > 0 {
                let (name, ver) = token.split_at(idx);
                let ver = ver.trim_start_matches('@');
                if !ver.is_empty() && ver != "latest" {
                    return Some((name.to_string(), Some(ver.to_string())));
                } else {
                    return Some((name.to_string(), None));
                }
            }
        }
        Some((token.to_string(), None))
    } else if let Some(idx) = token.find('@') {
        let (name, ver) = token.split_at(idx);
        let ver = ver.trim_start_matches('@');
        if !name.is_empty() {
            if !ver.is_empty() && ver != "latest" {
                Some((name.to_string(), Some(ver.to_string())))
            } else {
                Some((name.to_string(), None))
            }
        } else {
            None
        }
    } else {
        Some((token.to_string(), None))
    }
}

/// Parse `PyPI` package tokens: `package==1.2.3`, `package[extra]==1.2.3`
fn parse_pypi_token(token: &str) -> Option<(String, Option<String>)> {
    let lowered = token.to_ascii_lowercase();
    if let Some(idx) = lowered.find("==") {
        let (name, ver) = token.split_at(idx);
        let ver = ver.trim_start_matches('=');
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        if ver.is_empty() || ver.eq_ignore_ascii_case("latest") {
            return Some((name.to_string(), None));
        }
        return Some((name.to_string(), Some(ver.to_string())));
    }
    Some((token.to_string(), None))
}

// ── OSV API Types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct QueryRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    package: PackageInfo<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_token: Option<String>,
}

#[derive(Serialize)]
struct PackageInfo<'a> {
    name: &'a str,
    ecosystem: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    purl: Option<&'a str>,
}

#[derive(Deserialize)]
struct QueryResponse {
    #[serde(default)]
    vulns: Vec<Vuln>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct Vuln {
    id: String,
    #[serde(default)]
    summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── NPM Token Parsing Tests ──────────────────────────────────────────

    #[test]
    fn test_parse_npm_scoped_with_version() {
        assert_eq!(
            parse_npm_token("@scope/pkg@1.2.3"),
            Some(("@scope/pkg".into(), Some("1.2.3".into())))
        );
    }

    #[test]
    fn test_parse_npm_scoped_no_version() {
        assert_eq!(
            parse_npm_token("@scope/pkg"),
            Some(("@scope/pkg".into(), None))
        );
    }

    #[test]
    fn test_parse_npm_unscoped_with_version() {
        assert_eq!(
            parse_npm_token("react@18.3.1"),
            Some(("react".into(), Some("18.3.1".into())))
        );
    }

    #[test]
    fn test_parse_npm_unscoped_no_version() {
        assert_eq!(parse_npm_token("eslint"), Some(("eslint".into(), None)));
    }

    #[test]
    fn test_parse_npm_latest_is_none() {
        assert_eq!(
            parse_npm_token("react@latest"),
            Some(("react".into(), None))
        );
    }

    // ── PyPI Token Parsing Tests ──────────────────────────────────────────

    #[test]
    fn test_parse_pypi_exact_pin() {
        assert_eq!(
            parse_pypi_token("requests==2.32.3"),
            Some(("requests".into(), Some("2.32.3".into())))
        );
    }

    #[test]
    fn test_parse_pypi_no_version() {
        assert_eq!(
            parse_pypi_token("requests"),
            Some(("requests".into(), None))
        );
    }

    #[test]
    fn test_parse_pypi_latest_is_none() {
        assert_eq!(
            parse_pypi_token("requests==latest"),
            Some(("requests".into(), None))
        );
    }

    #[test]
    fn test_parse_pypi_empty_name() {
        assert_eq!(parse_pypi_token("==1.0.0"), None);
    }

    // ── Ecosystem Inference Tests ──────────────────────────────────────────

    #[test]
    fn test_parse_npm_flags_skipped() {
        let args = vec![
            "--dry-run".to_string(),
            "-y".to_string(),
            "some_pkg@1.2.3".to_string(),
        ];
        let result = parse_first_package_arg("npm", &args);
        assert_eq!(result, Some(("some_pkg".into(), Some("1.2.3".into()))));
    }

    #[test]
    fn test_parse_pypi_flags_skipped() {
        let args = vec!["--force".to_string(), "requests==2.28.0".to_string()];
        let result = parse_first_package_arg("PyPI", &args);
        assert_eq!(result, Some(("requests".into(), Some("2.28.0".into()))));
    }

    #[test]
    fn test_parse_empty_args() {
        let result = parse_first_package_arg("npm", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_only_flags() {
        let args = vec!["--yes".to_string(), "-D".to_string()];
        let result = parse_first_package_arg("npm", &args);
        assert!(result.is_none());
    }

    #[test]
    fn test_unknown_ecosystem_returns_none() {
        let args = vec!["some_pkg".to_string()];
        let result = parse_first_package_arg("cargo", &args);
        assert!(result.is_none());
    }

    // ── OsvChecker Construction Tests ──────────────────────────────────────

    #[test]
    fn test_osv_checker_new() {
        let checker = OsvChecker::new();
        assert!(checker.is_ok());
    }

    #[test]
    fn test_osv_checker_custom_endpoint() {
        let checker = OsvChecker::with_endpoint("http://localhost:8080/v1/query".to_string());
        assert!(checker.is_ok());
    }

    #[test]
    fn test_osv_error_display() {
        let err = OsvError::MaliciousPackage {
            name: "evil-pkg".to_string(),
            version: "1.0.0".to_string(),
            ecosystem: "npm".to_string(),
            advisories: "MAL-1234 — Known malware".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("Blocked malicious package"));
        assert!(msg.contains("evil-pkg"));
        assert!(msg.contains("MAL-1234"));
    }

    #[tokio::test]
    async fn test_deny_if_malicious_cmd_args_unknown_command() {
        // Unknown commands should skip the check entirely
        let result = deny_if_malicious_cmd_args("cargo", &["install".to_string()]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_deny_if_malicious_cmd_args_npm_no_args() {
        let result = deny_if_malicious_cmd_args("npx", &[]).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_osv_checker_default_does_not_panic() {
        // Default impl should always succeed, even if new() hypothetically fails
        let _checker = OsvChecker::default();
    }
}
