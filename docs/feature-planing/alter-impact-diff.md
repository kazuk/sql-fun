# ALTER TABLE Impact Diff Generator and Patch Series Proposal

*This is a planning document outlining a potential future feature of `sql-fun`.  
It has not been implemented, and no timeline or commitment has been established.*

## Overview

This feature proposes a toolchain extension for `sql-fun` that analyzes the effect of a given migration file (`up.sql`) on both the schema and the Rust codebase using `sql-fun`-annotated functions.

The tool will **not apply schema changes or fix code**, but instead:

- Parse `ALTER TABLE` statements from an input migration file
- Determine schema-level impacts
- Cross-reference with `sql-fun.develop.toml` to find affected queries and fields
- Generate a `patch series` — one patch per ALTER — representing the required diffs
- Leave all application decisions (apply, reject, fix, push PR) to the user

## Process

1. Parse migration file (multiple ALTER TABLE statements)
2. Track in-memory schema changes incrementally
3. For each ALTER:
   - Analyze affected columns/tables
   - Query TOML to find impacted Rust functions
   - Emit one `unified diff` patch per statement
4. Output patch series in order of ALTER appearance

## Technical Assumptions

- Based on current `sql-fun.develop.toml`
- Assumes sequential schema state updates (ALTER is cumulative)
- Does not mutate code or TOML in-place
- Uses only static analysis (no runtime DB access)

## Important

- 🛑 This tool **does not manage migrations**
- 🛠️ This tool **does not modify source files or apply patches**
- 🚦 CI breaks or PR flows caused by the patch are **intentional and external**

> It is the responsibility of the user to review, apply, and integrate suggested changes.

## Example Usage

```bash
sql-fun-cli suggest-fix --from-migration migrations/123_add_full_name.up.sql \
  --output patches/
```

```bash
cd my-project
git checkout -b sqlfun/migration-123
git apply patches/0001-rename-name.diff
git commit -am "Rename name to full_name per migration step 1"
# ...
```

Or:

```bash
gh pr create --base main --head sqlfun/migration-123 --title "SQL-Fun Migration Response Patches"
```

## Why It's in Feature Planning

This feature respects all core design philosophies of `sql-fun`:

| Principle             | Status                              |
|----------------------|-------------------------------------|
| No migration management | ✅ Not applying schema             |
| Just a Tool           | ✅ No action without user request   |
| Explicit choice       | ✅ All patch application is opt-in  |
| Static validation     | ✅ Full diff based on in-memory schema and TOML |

> 🔧 This feature is not yet implemented.  
> We're currently exploring feasibility and potential use cases.  
> Community interest or sponsorship may influence its development priority.
