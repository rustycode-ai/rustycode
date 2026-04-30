//! Network Egress Detection
//!
//! Extracts network destinations from shell commands and tool arguments.
//! Ported from goose's `security/egress_inspector.rs` as a standalone utility.
//!
//! Detects egress to:
//! - HTTP/HTTPS/FTP URLs
//! - Git SSH remotes
//! - S3/GCS cloud storage
//! - SCP/rsync targets
//! - SSH connections
//! - Docker registry pushes
//! - Package publish commands (npm, cargo)
//! - Generic network tools (nc, netcat, socat, etc.)
//!
//! # Example
//!
//! ```
//! use rustycode_tools::egress_detector::extract_destinations;
//!
//! let dests = extract_destinations("curl https://example.com/api/data");
//! assert_eq!(dests.len(), 1);
//! assert_eq!(dests[0].domain, "example.com");
//! assert_eq!(dests[0].kind, "url");
//! ```

use regex::Regex;
use std::collections::HashSet;

/// A detected network destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressDestination {
    /// Type of destination (url, `git_remote`, `s3_bucket`, etc.)
    pub kind: String,
    /// Full destination string
    pub destination: String,
    /// Extracted domain/hostname
    pub domain: String,
}

// ── Compiled Regex Patterns ──────────────────────────────────────────────────

static URL_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"(?i)(https?|ftp)://[^\s'"<>|;&)]+"#).unwrap());

static GIT_SSH_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"git@([^:]+):([^\s'"]+)"#).unwrap());

static S3_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"s3://([^/\s'"]+)(/[^\s'"]*)?"#).unwrap());

static GCS_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"gs://([^/\s'"]+)(/[^\s'"]*)?"#).unwrap());

static SCP_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?:scp|rsync)\s+.*?(?:\S+@)?([a-zA-Z0-9][\w.-]+):").unwrap()
});

static SSH_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"ssh\s+(?:-\w+\s+\S+\s+)*(?:\S+@)?([a-zA-Z0-9][\w.-]+)").unwrap()
});

static DOCKER_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"docker\s+(?:push|login)\s+(?:--[^\s]+\s+)*([^\s'"]+)"#).unwrap()
});

static GENERIC_NET_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(fetch|nc|ncat|netcat|ftp|sftp|socat|httpie|xh)\b[^\n]*?\b((?:[a-zA-Z0-9](?:[a-zA-Z0-9\-]*[a-zA-Z0-9])?\.)+[a-zA-Z]{2,})\b"
    ).unwrap()
});

static NPM_PUBLISH_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(?:^|[;&|]\s*|\n)\s*npm\s+publish(?:\s|$)").unwrap());

static CARGO_PUBLISH_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?:^|[;&|]\s*|\n)\s*cargo\s+publish(?:\s|$)").unwrap()
});

/// Extract all network destinations from a command string.
///
/// Returns a list of detected egress destinations with their type, full string,
/// and domain. Duplicate domains are deduplicated per call.
///
/// # Example
///
/// ```
/// use rustycode_tools::egress_detector::extract_destinations;
///
/// let dests = extract_destinations("curl https://api.example.com/v1/data");
/// assert!(!dests.is_empty());
/// assert_eq!(dests[0].kind, "url");
/// assert_eq!(dests[0].domain, "api.example.com");
/// ```
pub fn extract_destinations(command: &str) -> Vec<EgressDestination> {
    let mut destinations = Vec::new();
    let mut seen_domains: HashSet<String> = HashSet::new();

    // HTTP/HTTPS/FTP URLs
    for cap in URL_RE.find_iter(command) {
        let url = cap.as_str().to_string();
        if let Some(domain) = extract_domain_from_url(&url) {
            if seen_domains.insert(domain.to_lowercase()) {
                destinations.push(EgressDestination {
                    kind: "url".to_string(),
                    destination: url,
                    domain,
                });
            }
        }
    }

    // Git SSH remotes
    for cap in GIT_SSH_RE.captures_iter(command) {
        let domain = cap[1].to_string();
        if seen_domains.insert(domain.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "git_remote".to_string(),
                destination: format!("git@{}:{}", &domain, &cap[2]),
                domain,
            });
        }
    }

    // S3 buckets
    for cap in S3_RE.captures_iter(command) {
        let bucket = cap[1].to_string();
        let domain = format!("{bucket}.s3.amazonaws.com");
        if seen_domains.insert(domain.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "s3_bucket".to_string(),
                destination: cap[0].to_string(),
                domain,
            });
        }
    }

    // GCS buckets
    for cap in GCS_RE.captures_iter(command) {
        let bucket = cap[1].to_string();
        let domain = format!("{bucket}.storage.googleapis.com");
        if seen_domains.insert(domain.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "gcs_bucket".to_string(),
                destination: cap[0].to_string(),
                domain,
            });
        }
    }

    // SCP/rsync targets
    for cap in SCP_RE.captures_iter(command) {
        let host = cap[1].to_string();
        if seen_domains.insert(host.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "scp_target".to_string(),
                destination: cap[0].to_string(),
                domain: host,
            });
        }
    }

    // SSH connections
    for cap in SSH_RE.captures_iter(command) {
        let host = cap[1].to_string();
        if !host.starts_with('-') && seen_domains.insert(host.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "ssh_target".to_string(),
                destination: cap[0].to_string(),
                domain: host,
            });
        }
    }

    // Docker registry push/login
    for cap in DOCKER_RE.captures_iter(command) {
        let target = cap[1].to_string();
        let domain = target.split('/').next().unwrap_or(&target).to_string();
        if seen_domains.insert(domain.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "docker_registry".to_string(),
                destination: target,
                domain,
            });
        }
    }

    // Generic network tools (nc, netcat, socat, etc.)
    for cap in GENERIC_NET_RE.captures_iter(command) {
        let domain = cap[2].to_string();
        if seen_domains.insert(domain.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "generic_network".to_string(),
                destination: cap[0].to_string(),
                domain,
            });
        }
    }

    // npm publish
    if NPM_PUBLISH_RE.is_match(command) {
        let domain = "registry.npmjs.org".to_string();
        if seen_domains.insert(domain.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "package_publish".to_string(),
                destination: "npm publish".to_string(),
                domain,
            });
        }
    }

    // cargo publish
    if CARGO_PUBLISH_RE.is_match(command) {
        let domain = "crates.io".to_string();
        if seen_domains.insert(domain.to_lowercase()) {
            destinations.push(EgressDestination {
                kind: "package_publish".to_string(),
                destination: "cargo publish".to_string(),
                domain,
            });
        }
    }

    destinations
}

/// Extract domain from a URL string.
///
/// Handles user:pass@host, `IPv6` brackets, and port numbers.
pub fn extract_domain_from_url(url: &str) -> Option<String> {
    let after_scheme = url.find("://").and_then(|i| url.get(i + 3..))?;
    let authority = after_scheme.split('/').next()?;
    let host_port = authority.split('@').next_back()?;
    let host = if host_port.contains('[') {
        // IPv6
        host_port
            .split(']')
            .next()
            .map(|s| s.trim_start_matches('['))?
    } else {
        host_port.split(':').next()?
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Format destinations as a human-readable summary.
pub fn format_destinations(destinations: &[EgressDestination]) -> String {
    if destinations.is_empty() {
        return "No egress destinations detected".to_string();
    }
    destinations
        .iter()
        .map(|d| format!("[{}] {}", d.kind, d.destination))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Check if a command has any egress destinations.
pub fn has_egress(command: &str) -> bool {
    !extract_destinations(command).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_extraction() {
        let dests = extract_destinations("curl https://example.com/api/data");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "url");
        assert_eq!(dests[0].domain, "example.com");
        assert_eq!(dests[0].destination, "https://example.com/api/data");
    }

    #[test]
    fn test_multiple_urls() {
        let dests = extract_destinations(
            "curl https://api.example.com/v1 && wget http://backup.example.com/file",
        );
        assert_eq!(dests.len(), 2);
    }

    #[test]
    fn test_git_ssh_remote() {
        let dests = extract_destinations("git remote add origin git@github.com:personal/repo.git");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "git_remote");
        assert_eq!(dests[0].domain, "github.com");
    }

    #[test]
    fn test_s3_bucket() {
        let dests = extract_destinations("aws s3 cp data.csv s3://my-bucket/path/data.csv");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "s3_bucket");
        assert_eq!(dests[0].domain, "my-bucket.s3.amazonaws.com");
    }

    #[test]
    fn test_gcs_bucket() {
        let dests = extract_destinations("gsutil cp data.csv gs://my-bucket/path/data.csv");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "gcs_bucket");
        assert_eq!(dests[0].destination, "gs://my-bucket/path/data.csv");
        assert_eq!(dests[0].domain, "my-bucket.storage.googleapis.com");
    }

    #[test]
    fn test_scp_target() {
        let dests = extract_destinations("scp file.txt user@remote.example.com:/tmp/file.txt");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "scp_target");
        assert_eq!(dests[0].domain, "remote.example.com");
    }

    #[test]
    fn test_rsync_target() {
        let dests = extract_destinations("rsync -av ./dist/ deploy@prod.example.com:/var/www/");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "scp_target");
        assert_eq!(dests[0].domain, "prod.example.com");
    }

    #[test]
    fn test_ssh_target() {
        let dests = extract_destinations("ssh user@bastion.example.com");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "ssh_target");
        assert_eq!(dests[0].domain, "bastion.example.com");
    }

    #[test]
    fn test_ssh_with_options() {
        let dests = extract_destinations("ssh -i key.pem ec2-user@10.0.0.1");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "ssh_target");
        assert_eq!(dests[0].domain, "10.0.0.1");
    }

    #[test]
    fn test_docker_push() {
        let dests = extract_destinations("docker push registry.example.com/myapp:latest");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "docker_registry");
        assert_eq!(dests[0].domain, "registry.example.com");
    }

    #[test]
    fn test_docker_login() {
        let dests = extract_destinations("docker login ghcr.io");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "docker_registry");
        assert_eq!(dests[0].domain, "ghcr.io");
    }

    #[test]
    fn test_generic_network_nc() {
        let dests = extract_destinations("nc data.exfil.io 9999");
        assert!(dests
            .iter()
            .any(|d| d.kind == "generic_network" && d.domain == "data.exfil.io"));
    }

    #[test]
    fn test_npm_publish() {
        assert_eq!(extract_destinations("npm publish").len(), 1);
        assert_eq!(extract_destinations("cd pkg && npm publish").len(), 1);
    }

    #[test]
    fn test_cargo_publish() {
        let dests = extract_destinations("cargo publish");
        assert!(dests
            .iter()
            .any(|d| d.kind == "package_publish" && d.domain == "crates.io"));
    }

    #[test]
    fn test_no_false_positives() {
        assert_eq!(extract_destinations("ls -la /tmp").len(), 0);
        assert_eq!(extract_destinations("cargo build --release").len(), 0);
        assert_eq!(extract_destinations("git status").len(), 0);
    }

    #[test]
    fn test_npm_publish_false_negatives() {
        assert_eq!(extract_destinations("echo 'npm publish'").len(), 0);
        assert_eq!(extract_destinations("cat npm_publish_guide.md").len(), 0);
    }

    #[test]
    fn test_deduplication() {
        // Same URL twice should only produce one destination
        let dests =
            extract_destinations("curl https://example.com/a && curl https://example.com/b");
        assert_eq!(dests.len(), 1);
    }

    #[test]
    fn test_domain_from_url_basic() {
        assert_eq!(
            extract_domain_from_url("https://example.com/path"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_url_with_auth() {
        assert_eq!(
            extract_domain_from_url("https://user:pass@example.com/path"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_url_with_port() {
        assert_eq!(
            extract_domain_from_url("https://example.com:8080/path"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_domain_from_url_no_scheme() {
        assert_eq!(extract_domain_from_url("not-a-url"), None);
    }

    #[test]
    fn test_has_egress() {
        assert!(has_egress("curl https://example.com"));
        assert!(!has_egress("ls -la"));
    }

    #[test]
    fn test_format_destinations_empty() {
        assert_eq!(format_destinations(&[]), "No egress destinations detected");
    }

    #[test]
    fn test_format_destinations() {
        let dests = vec![EgressDestination {
            kind: "url".to_string(),
            destination: "https://example.com".to_string(),
            domain: "example.com".to_string(),
        }];
        let formatted = format_destinations(&dests);
        assert!(formatted.contains("[url]"));
        assert!(formatted.contains("https://example.com"));
    }

    #[test]
    fn test_ftp_url() {
        let dests = extract_destinations("ftp://files.example.com/data.csv");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "url");
        assert_eq!(dests[0].domain, "files.example.com");
    }
}
