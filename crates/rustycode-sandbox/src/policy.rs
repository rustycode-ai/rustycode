use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    pub read_paths: Vec<PathBuf>,
    pub write_paths: Vec<PathBuf>,
    pub network: NetworkAccess,
    pub env_passthrough: Vec<String>,
    pub max_memory_mb: Option<u32>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkAccess {
    Denied,
    Allowed,
}

impl SandboxPolicy {
    /// Restrictive: read/write workspace only, no network.
    pub fn restrictive(workspace_root: &std::path::Path) -> Self {
        Self {
            read_paths: vec![workspace_root.to_path_buf()],
            write_paths: vec![workspace_root.to_path_buf()],
            network: NetworkAccess::Denied,
            env_passthrough: vec![
                "PATH".into(),
                "HOME".into(),
                "LANG".into(),
                "TERM".into(),
            ],
            max_memory_mb: Some(512),
            timeout_secs: Some(120),
        }
    }

    /// Permissive: read anywhere, write workspace, network allowed.
    pub fn permissive(workspace_root: &std::path::Path) -> Self {
        Self {
            read_paths: vec![PathBuf::from("/")],
            write_paths: vec![workspace_root.to_path_buf()],
            network: NetworkAccess::Allowed,
            env_passthrough: vec![],
            max_memory_mb: None,
            timeout_secs: Some(300),
        }
    }
}
