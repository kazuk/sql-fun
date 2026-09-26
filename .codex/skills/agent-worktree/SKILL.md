---
name: agent-worktree
description: Create a linked Git worktree for this repository from an agent session. Use when work must occur in a separate worktree; do not use for ordinary branch changes in the current workspace.
---

# Agent worktree

Use `agent-worktree-add` instead of `git worktree add`. The helper fixes every
new worktree below `$GIT_ROOT/.worktrees/`, where it remains visible to the
host environment.

Run this from the repository root after fetching the required base ref:

```bash
git fetch origin <base-branch>
agent-worktree-add <branch-name> origin/<base-branch>
```

The helper prints the created worktree path. It accepts a valid new branch name
and an optional start point; it does not accept a destination path.

Use ordinary Git commands from within the created worktree. When it is no
longer needed and has no uncommitted work, remove it from the main worktree:

```bash
git worktree remove "$GIT_ROOT/.worktrees/<branch-name>"
```
