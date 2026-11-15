# Next-major compatibility migration specifications

When a breaking public API cleanup is deferred to preserve the current release
intent, record the temporary compatibility debt in a Markdown specification
under the affected crate's `next-major/` directory.

This directory is a work queue for the **next major release**, not a permanent
documentation archive. Keep each specification focused on one migration and
name it after the affected API or compatibility layer, for example
`sql-fun-core/next-major/remove-legacy-type-adapter.md`.

## When to create a specification

Create the specification in the same PR that introduces or deliberately retains
a compatibility workaround for a future breaking release. Do not create a
specification for an ordinary compatible change that has no planned migration.

Include enough information for a later contributor or agent to complete the
migration without reconstructing the original decision:

- **Reason:** why the compatibility behavior must remain for the current release.
- **Next-major change:** the intended public API or behavior after the breaking change.
- **Removal/simplification:** the concrete compatibility code to remove or simplify.
- **References:** affected APIs, relevant tests, and the introducing PR or issue.
- **Completion criteria:** observable conditions that establish the migration is done.

Use a simple Markdown file with those headings; no special format, index,
automation, or validation infrastructure is required.

## Completing a migration

When a crate's release intent is raised to `major`, all outstanding
`next-major/*.md` specifications for that crate must be implemented and
resolved as part of that change. Remove each specification in the same change
that applies its migration; do not leave deferred compatibility work for later
in the major release cycle. Git history and the PR record retain the completed
migration decisions.

Keep the ordinary SemVer release-intent check and its declared release level
as the authority for whether the breaking change is permitted. A migration
specification does not exempt changes from SemVer validation and does not
automatically authorize implementation.
