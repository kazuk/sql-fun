---
name: old-source-review
description: Review one old Rust source file and create an evidence-bearing maintenance issue for a maintainer's disposition decision. Use for the repository's periodic old-source maintenance workflow.
---

# Old source review

Create one English maintenance issue from evidence about one Rust source file.
The investigation records facts and a suggested disposition; it does not alter
the reviewed source or implement a recommendation.

## Prerequisites

Run this skill from the repository root with `bash`, `git`, `jq`, and an
authenticated `gh` client available. Creating the issue requires repository
write permission.

## Procedure

1. Invoking this repository-local skill selects its containing repository and
   authorizes inspection under the deterministic `.local/bin/old-source` policy,
   semantic investigation, and creation of one maintenance issue. No additional
   repository or target confirmation is required. Collect the first candidate:

   ```bash
   .codex/skills/old-source-review/scripts/collect-evidence.sh --oldest
   ```

2. Adopt the returned `target` and `evidence_commit`. The first result from
   `.local/bin/old-source` is authoritative. Do not filter, rerank, exclude
   previously reviewed files, or substitute a replacement candidate. Stop if
   collection fails. `--suggest-candidate` remains a non-selecting manual mode;
   the unattended procedure uses `--oldest`.

3. Use the returned JSON facts for the target, tracked-file
   validation, and last-modifying commit. Do not recreate those values in the
   investigation. Inspect the selected source at the returned `evidence_commit`
   with `git show <evidence_commit>:<target>`, so source inspection and literal
   usage evidence share the same committed snapshot.

4. Identify meaningful definitions in that snapshot. Then
   pass every selected definition name to the helper's lookup mode:

   ```bash
   .codex/skills/old-source-review/scripts/collect-evidence.sh \
     --target <path> --evidence-commit <evidence_commit> \
     --definition <name> [--definition <name> ...]
   ```

   The returned literal matches are evidence only. Determine whether they are
   definitions, production use, test-only use, public exposure, trait-driven or
   indirect use, or ambiguous by inspecting the relevant code. Report
   uncertainty instead of inferring a use from a name match.

5. Investigate adjacent concepts and assess why the definitions have remained
   stable. Consider **Keep**, **Consolidate**, **Remove / simplify**, **Expand
   usage**, and **Further investigation**. File age and low use count alone do
   not justify a change.

6. Obtain the current required maintenance headings without manually loading
   the template:

   ```bash
   .codex/skills/old-source-review/scripts/create-maintenance-issue.sh \
     --required-headings
   ```

   Draft an English issue body with those headings, plus `## Pending`. Include
   the target file, last-modifying commit, definitions, usage evidence,
   adjacent concepts, assessment, suggested disposition and rationale,
   uncertainties, and the maintainer decision required in `Pending`.

7. Save that body to a temporary file and create the issue once:

   ```bash
   .codex/skills/old-source-review/scripts/create-maintenance-issue.sh \
     --repository-context \
     --title "[Maintenance] <target>: <suggested disposition>" \
     --body-file <path>
   ```

   The helper resolves the destination from the containing repository's `origin`
   remote (GitHub HTTPS or SSH), including a fork's own origin. It never selects
   an upstream or uses ambient `GH_REPO` or the caller's current directory.
   Missing, unsupported, or unresolvable origin stops creation without fallback.
   `--repo` remains available for explicit manual helper use.

   The helper validates the template headings, appends the fixed Keep follow-up
   notice, and applies `not-ready`. If creation fails or its result is
   uncertain, stop and report the exact human action required; never retry a
   possibly successful creation.

## Lifecycle

The maintainer decides the disposition. A **Keep** decision still requires a
committed update to the reviewed source, such as a review comment containing
the decision, rationale, and issue link. That update advances the Git date used
by `.local/bin/old-source`; updating only the GitHub issue or filesystem time
does not. Record that follow-up commit in the issue before closing it.

For a disposition requiring implementation, the maintainer records enough
direction to make the task executable and removes `not-ready`. Do not make that
implementation as part of this investigation skill.
