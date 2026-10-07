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

1. Select the target with the deterministic evidence helper:

   ```bash
   .codex/skills/old-source-review/scripts/collect-evidence.sh
   ```

   To review an explicitly selected source instead, pass it exactly:

   ```bash
   .codex/skills/old-source-review/scripts/collect-evidence.sh --target <path>
   ```

   With no target, the helper selects the first result from
   `.local/bin/old-source`. That command evaluates all tracked Rust sources;
   do not add a subset filter or substitute a different candidate.

2. Use the returned JSON facts for the target, candidate basis, tracked-file
   validation, and last-modifying commit. Do not recreate those values in the
   investigation.

3. Inspect the selected source and identify its meaningful definitions. Then
   pass every selected definition name to the helper's lookup mode:

   ```bash
   .codex/skills/old-source-review/scripts/collect-evidence.sh \
     --target <path> --definition <name> [--definition <name> ...]
   ```

   The returned literal matches are evidence only. Determine whether they are
   definitions, production use, test-only use, public exposure, trait-driven or
   indirect use, or ambiguous by inspecting the relevant code. Report
   uncertainty instead of inferring a use from a name match.

4. Investigate adjacent concepts and assess why the definitions have remained
   stable. Consider **Keep**, **Consolidate**, **Remove / simplify**, **Expand
   usage**, and **Further investigation**. File age and low use count alone do
   not justify a change.

5. Obtain the current required maintenance headings without manually loading
   the template:

   ```bash
   .codex/skills/old-source-review/scripts/create-maintenance-issue.sh \
     --required-headings
   ```

   Draft an English issue body with those headings, plus `## Pending`. Include
   the target file, last-modifying commit, definitions, usage evidence,
   adjacent concepts, assessment, suggested disposition and rationale,
   uncertainties, and the maintainer decision required in `Pending`.

6. Save that body to a temporary file and create the issue once:

   ```bash
   .codex/skills/old-source-review/scripts/create-maintenance-issue.sh \
     --title "[Maintenance] <target>: <suggested disposition>" \
     --body-file <path>
   ```

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
