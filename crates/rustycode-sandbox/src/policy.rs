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
        // Canonicalize to resolve symlinks — prevents escape via symlink chains
        let root = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        Self {
            read_paths: vec![root.clone()],
            write_paths: vec![root],
            network: NetworkAccess::Denied,
            env_passthrough: vec!["PATH".into(), "HOME".into(), "LANG".into(), "TERM".into()],
            max_memory_mb: Some(512),
            timeout_secs: Some(120),
        }
    }

    /// Permissive: read anywhere, write workspace, network allowed.
    pub fn permissive(workspace_root: &std::path::Path) -> Self {
        // Canonicalize to resolve symlinks — prevents escape via symlink chains
        let root = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        Self {
            read_paths: vec![PathBuf::from("/")],
            write_paths: vec![root],
            network: NetworkAccess::Allowed,
            env_passthrough: vec![],
            max_memory_mb: None,
            timeout_secs: Some(300),
        }
    }

    /// Build a policy from allowed/denied paths and timeout, matching
    /// the fields in `SandboxConfig` without depending on that type.
    pub fn from_config(
        allowed_paths: Option<&[PathBuf]>,
        denied_paths: &[PathBuf],
        timeout_secs: Option<u64>,
        workspace_root: &std::path::Path,
    ) -> Self {
        let mut read_paths = allowed_paths.map(Vec::from).unwrap_or_default();

        if read_paths.is_empty() {
            read_paths.push(PathBuf::from("/"));
        }

        // Ensure workspace root is always readable
        if !read_paths.contains(&workspace_root.to_path_buf()) {
            read_paths.push(workspace_root.to_path_buf());
        }

        let mut write_paths = read_paths.clone();

        // Remove denied paths from write access
        write_paths.retain(|p| !denied_paths.contains(p));

        Self {
            read_paths,
            write_paths,
            network: NetworkAccess::Denied,
            env_passthrough: vec!["PATH".into(), "HOME".into(), "LANG".into(), "TERM".into()],
            max_memory_mb: Some(512),
            timeout_secs: timeout_secs.or(Some(120)),
        }
    }
}
