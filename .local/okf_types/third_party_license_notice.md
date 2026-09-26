---
type: Concept Type Definition
defined_type: Third Party License Notice
---

# Third Party License Notice

## Type Description

`Third Party License Notice` records the provenance, license review, and
required attribution for a third-party resource vendored into this repository.

## Frontmatter Fields

- `sources`: REQUIRED List, imported source records.
  - `url`: REQUIRED Text, immutable source URL when available.
  - `revision`: OPTIONAL Text, source revision used to retrieve the resource.
- `resource_path`: REQUIRED Text, repository-relative path of the vendored
  resource.
- `content_sha256`: REQUIRED Text, SHA-256 hash of the vendored resource.
- `detected_license`: REQUIRED Text, mechanically detected license information,
  or `none` when no information was found. This field is unreviewed input.
- `review_status`: REQUIRED Text, `pending` until a reviewer has checked the
  applicable primary license and notice material, then `reviewed`.
- `reviewed_license`: OPTIONAL Text, license conclusion after review.

## Markdown Content

- `Review` heading: REQUIRED, records the primary license/notice sources that
  were reviewed and any corrections to mechanically collected data.
- `Notice Text` heading: REQUIRED, contains the finalized text to append to
  `THIRD_PARTY_NOTICES.md`.

## Placement

Place each concept adjacent to its vendored resource and name it
`<resource-name>.license-notice.md`.

## Lifecycle

The generated scaffold is pending until the primary license and notice sources
are reviewed. Only a reviewed concept may supply text for
`THIRD_PARTY_NOTICES.md`.

## Agent Skills

Use the repository [vendor-third-party skill](../../.codex/skills/vendor-third-party/SKILL.md)
to create and complete concepts of this type.
