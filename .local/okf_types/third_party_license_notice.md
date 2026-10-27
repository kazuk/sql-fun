---
type: Concept Type Definition
defined_type: Third Party License Notice
---

# Third Party License Notice

## Type Description

`Third Party License Notice` records the provenance, automated license
verification and required attribution for a third-party resource vendored into
this repository.

## Frontmatter Fields

- `sources`: REQUIRED List, imported source records.
  - `url`: REQUIRED Text, immutable source URL when available.
  - `revision`: OPTIONAL Text, source revision used to retrieve the resource.
- `resource_path`: REQUIRED Text, repository-relative path of the vendored
  resource.
- `content_sha256`: REQUIRED Text, SHA-256 hash of the vendored resource.
- `detected_license`: REQUIRED Text, mechanically detected license information,
  or `none` when no information was found. This field is unreviewed input.
- `automated_verification`: REQUIRED Object, records the result of the
  automated workflow.
  - `status`: REQUIRED Text, `pending` until the workflow has checked its
    collected information against the recorded source and license material, then
    `completed`.
  - `license`: REQUIRED Text, license information established by that automated
    verification, or `unknown` while pending.

Automated verification is not legal review or approval by the repository owner
or maintainers. It records only the outcome of this workflow and does not make
an assertion about legal sufficiency.

## Markdown Content

- `Automated verification` heading: REQUIRED, records the primary
  license/notice sources checked by the workflow and any corrections to
  mechanically collected data.
- `Notice Text` heading: REQUIRED, contains the finalized text to append to
  `THIRD_PARTY_NOTICES.md`.

## Placement

Place each concept adjacent to its vendored resource and name it
`<resource-name>.license-notice.md`.

## Lifecycle

The generated scaffold is pending until automated verification records a
completed result. Only a concept with completed automated verification may
supply text for `THIRD_PARTY_NOTICES.md`.

## Agent Skills

Use the repository [vendor-third-party skill](../../.codex/skills/vendor-third-party/SKILL.md)
to create and complete concepts of this type.
