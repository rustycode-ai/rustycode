# rustycode-git

Git operations and worktree management for RustyCode.

## Purpose

Provides git integration for repository operations (status, commit, branch, push, etc.) and worktree management for isolated development branches. Enables multi-branch parallel work without conflicts.

## Key Types

- `GitRepository` — Interface to a git repository
- `WorktreeManager` — Create and manage worktrees
- `Worktree` — Isolated working directory
- `GitStatus` — Repository status (modified files, staged changes)
- `CommitMessage` — Structured commit with title, body, footer
- `Branch` — Git branch with tracking info

## Public API

```rust
use rustycode_git::{GitRepository, WorktreeManager};

// Open repository
let repo = GitRepository::open(".")?;

// Get status
let status = repo.status()?;
println!("Modified: {}", status.modified_files.len());

// Create and use worktree
let wm = WorktreeManager::new(&repo)?;
let worktree = wm.create_worktree("feature/my-feature")?;

// Work in worktree
std::env::set_current_dir(worktree.path())?;

// Make changes and commit
repo.add_all()?;
repo.commit("feat: implement feature")?;

// Push to remote
repo.push("origin", "feature/my-feature")?;
```

## Operations

**Repository:**
- status, add, commit, push, pull, branch, merge, rebase, log

**Worktrees:**
- create_worktree, list_worktrees, delete_worktree, checkout
- Automatic cleanup on drop (can disable)

**History:**
- log, show_commit, blame, diff
- Filter by author, date, message

## Worktree Benefits

- Parallel feature development without branch switching
- Isolated builds for different branches
- No stale state from branch switching
- Automatic cleanup when dropping worktree

## Dependencies

- `git2` or equivalent — Underlying git library
- `tempfile` — Temporary worktree directories
- `tokio` — Async execution (optional)
- `anyhow` — Error handling

## Architecture Notes

Git operations wrap underlying library (git2-rs) with RustyCode-specific patterns. Worktrees are created in temporary directories and cleaned up on drop.

Commit messages support conventional format (feat, fix, docs, test, etc.) for automatic changelog generation.

All operations handle common git errors (merge conflicts, auth failures, divergent branches) with clear error messages.

## Testing

Tests use temporary git repositories created on-the-fly. No external git server needed. Tests verify all operations and error cases.

## See Also

- `rustycode-guard` — Restricts dangerous git operations (push --force)
- `rustycode-tools` — Git tool implementation (uses this)
- `rustycode-core` — Session recovery uses git worktrees
