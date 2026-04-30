---
name: worktree
description: Manage isolated development environments using git worktrees. Use when testing, managing concurrent tasks, or isolating sandbox environments.
license: MIT
metadata:
  version: "1.0"
  author: rustycode
---

# Worktree Skill

The Worktree Skill allows the agent to manage isolated development environments using git worktrees. This is useful for testing, concurrent task management, or sandbox environments.

## Capabilities
- **Create**: Setup a new worktree for a specific task.
- **List**: View all managed worktrees.
- **Delete**: Remove a worktree when the task is complete.

## Workflow
1. When starting a complex, isolated task, use `worktree_create`.
2. Perform development within that environment.
3. Once the task is finished and merged/committed, use `worktree_delete` to clean up.

## Tools
- `worktree_create`: Create a new worktree instance.
- `worktree_list`: List current worktrees.
- `worktree_delete`: Remove a worktree.
