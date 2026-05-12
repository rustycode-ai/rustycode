//! Workspace module - File system and checkpoint management

pub mod checkpoint;
pub mod formatter;
pub mod hints;
pub mod paths;
pub mod snapshot;
pub mod worktree;

// Re-export key types for backward compatibility
pub use checkpoint::{
    CheckpointConfig, CheckpointId, CheckpointManager, RestoreMode, StorageBasedCheckpointStore,
    WorkspaceCheckpoint,
};
pub use formatter::{detect_formatters, format_file, DetectedFormatter};
pub use hints::{
    build_gitignore, default_hints_filenames, find_git_root, load_hint_files,
    SubdirectoryHintTracker,
};
pub use paths::AppPaths;
pub use snapshot::{FileSnapshot, FileSnapshotManager, SnapshotGroup, UndoResult};
pub use worktree::{
    EnterWorktreeParams, ExitWorktreeParams, WorktreeCreateParams, WorktreeDeleteParams,
    WorktreeListParams,
};
