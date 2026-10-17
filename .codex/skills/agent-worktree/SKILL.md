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

After creating or entering a worktree, set `SQL_FUN_HOME` to the absolute path
of that worktree's `sql-fun-home` directory. Do not inherit the parent checkout's
setting. Run this from inside the target worktree:

```bash
export SQL_FUN_HOME="$(git rev-parse --show-toplevel)/sql-fun-home"
```

The helper cannot update its calling shell's environment. If the agent starts
independent shells, repeat this export in every invocation that runs builds,
tests, or CLI commands; an export in one invocation does not persist into others.

Use ordinary Git commands from within the created worktree. When it is no
longer needed and has no uncommitted work, remove it from the main worktree:

```bash
git worktree remove "$GIT_ROOT/.worktrees/<branch-name>"
```
