use super::{InspectionAction, InspectionResult, ToolCallInfo, ToolInspector};
use crate::ToolContext;
use rustycode_protocol::tool_names as tn;

/// Detects and logs network destinations in tool calls (especially bash commands).
///
/// Inspired by goose's egress inspector, this scans bash/web tool arguments
/// for URLs, git remotes, S3/GCS buckets, SCP/SSH targets, Docker registries,
/// and package publish commands. Detected destinations are logged for audit
/// purposes and can optionally require approval.
///
/// Uses the standalone `egress_detector` module for pattern extraction.
///
/// This inspector always **allows** the call but logs the egress destinations
/// at INFO level for security auditing.
pub struct EgressInspector {
    /// Whether to require approval for detected egress
    require_approval: bool,
}

impl EgressInspector {
    pub const fn new() -> Self {
        Self {
            require_approval: false,
        }
    }

    /// Create an egress inspector that requires approval for network calls.
    pub const fn with_approval_required() -> Self {
        Self {
            require_approval: true,
        }
    }
}

impl Default for EgressInspector {
    fn default() -> Self {
        Self::new()
    }
}

fn is_shell_tool(name: &str) -> bool {
    matches!(name, "Bash" | "shell" | "execute_command" | "run_command")
}

impl ToolInspector for EgressInspector {
    fn name(&self) -> &'static str {
        "egress"
    }

    fn inspect(
        &self,
        call: &ToolCallInfo,
        _history: &[ToolCallInfo],
        _ctx: &ToolContext,
    ) -> InspectionResult {
        if !is_shell_tool(&call.name) && call.name != tn::WEB_FETCH {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: "Not a network-capable tool".to_string(),
                confidence: 1.0,
                inspector_name: "egress".to_string(),
                finding_id: None,
            };
        }

        // Extract command or URL from arguments
        let text = if is_shell_tool(&call.name) {
            call.arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            call.arguments
                .get("url")
                .or_else(|| call.arguments.get("endpoint"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };

        if text.is_empty() {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: "No command/URL to inspect".to_string(),
                confidence: 1.0,
                inspector_name: "egress".to_string(),
                finding_id: None,
            };
        }

        let destinations = crate::egress_detector::extract_destinations(&text);

        if destinations.is_empty() {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: "No egress destinations detected".to_string(),
                confidence: 1.0,
                inspector_name: "egress".to_string(),
                finding_id: None,
            };
        }

        let dest_summary = destinations
            .iter()
            .map(|d| format!("{} ({})", d.destination, d.kind))
            .collect::<Vec<_>>()
            .join(", ");

        tracing::info!(
            "[egress] {} destinations detected: {}",
            destinations.len(),
            dest_summary
        );

        if self.require_approval {
            InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::RequireApproval(Some(format!(
                    "Network egress detected: {dest_summary}"
                ))),
                reason: format!("Egress destinations detected: {dest_summary}"),
                confidence: 1.0,
                inspector_name: "egress".to_string(),
                finding_id: Some("EGRESS-001".to_string()),
            }
        } else {
            InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Allow,
                reason: format!("Egress detected (logged): {dest_summary}"),
                confidence: 1.0,
                inspector_name: "egress".to_string(),
                finding_id: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir())
    }

    fn make_call(name: &str, args: serde_json::Value) -> ToolCallInfo {
        ToolCallInfo::new("test-id", name, args)
    }

    #[test]
    fn test_egress_extracts_url() {
        let dests =
            crate::egress_detector::extract_destinations("curl https://example.com/api/data");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].domain, "example.com");
        assert_eq!(dests[0].kind, "url");
    }

    #[test]
    fn test_egress_extracts_git_remote() {
        let dests = crate::egress_detector::extract_destinations(
            "git remote add origin git@github.com:user/repo.git",
        );
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].domain, "github.com");
        assert_eq!(dests[0].kind, "git_remote");
    }

    #[test]
    fn test_egress_extracts_s3() {
        let dests = crate::egress_detector::extract_destinations(
            "aws s3 cp data.csv s3://my-bucket/path/data.csv",
        );
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "s3_bucket");
    }

    #[test]
    fn test_egress_detects_npm_publish() {
        assert_eq!(
            crate::egress_detector::extract_destinations("npm publish").len(),
            1
        );
        assert_eq!(
            crate::egress_detector::extract_destinations("cd pkg && npm publish").len(),
            1
        );
        // Should not detect false positives
        assert_eq!(
            crate::egress_detector::extract_destinations("echo 'npm publish'").len(),
            0
        );
    }

    #[test]
    fn test_egress_detects_cargo_publish() {
        assert_eq!(
            crate::egress_detector::extract_destinations("cargo publish").len(),
            1
        );
        assert_eq!(
            crate::egress_detector::extract_destinations("cargo publish --dry-run").len(),
            1
        );
    }

    #[test]
    fn test_egress_detects_ssh() {
        let dests = crate::egress_detector::extract_destinations("ssh user@bastion.example.com");
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "ssh_target");
        assert_eq!(dests[0].domain, "bastion.example.com");
    }

    #[test]
    fn test_egress_detects_docker_push() {
        let dests = crate::egress_detector::extract_destinations(
            "docker push registry.example.com/myapp:latest",
        );
        assert_eq!(dests.len(), 1);
        assert_eq!(dests[0].kind, "docker_registry");
        assert_eq!(dests[0].domain, "registry.example.com");
    }

    #[test]
    fn test_egress_no_destinations_for_local_command() {
        assert_eq!(
            crate::egress_detector::extract_destinations("ls -la /tmp").len(),
            0
        );
        assert_eq!(
            crate::egress_detector::extract_destinations("cargo build --release").len(),
            0
        );
    }

    #[test]
    fn test_egress_inspector_allows_no_egress() {
        let inspector = EgressInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "ls -la"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
        assert_eq!(result.inspector_name, "egress");
    }

    #[test]
    fn test_egress_inspector_logs_url_egress() {
        let inspector = EgressInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "curl https://example.com/api"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
        assert!(result.reason.contains("example.com"));
    }

    #[test]
    fn test_egress_inspector_approval_mode() {
        let inspector = EgressInspector::with_approval_required();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "curl https://example.com/api"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
        assert_eq!(result.finding_id, Some("EGRESS-001".to_string()));
    }

    #[test]
    fn test_egress_inspector_skips_read_tools() {
        let inspector = EgressInspector::new();
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
        assert!(result.reason.contains("Not a network-capable tool"));
    }

    #[test]
    fn test_egress_detects_multiple_destinations() {
        let dests = crate::egress_detector::extract_destinations(
            "curl https://api.example.com/data && git push git@github.com:user/repo.git",
        );
        assert!(dests.len() >= 2);
        let kinds: Vec<&str> = dests.iter().map(|d| d.kind.as_str()).collect();
        assert!(kinds.contains(&"url"));
        assert!(kinds.contains(&"git_remote"));
    }

    #[test]
    fn test_extract_domain_from_url() {
        use crate::egress_detector::extract_domain_from_url;
        assert_eq!(
            extract_domain_from_url("https://example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_domain_from_url("https://user:pass@example.com:8080/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_domain_from_url("ftp://files.example.com"),
            Some("files.example.com".to_string())
        );
    }
}
