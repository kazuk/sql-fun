# Maintaining `vendor-third-party`

This README is for maintainers of the skill. It is not part of the execution
procedure.

## Scripts

- [01-fetch.sh](scripts/01-fetch.sh) downloads an HTTPS source URL to a
  repository-relative destination.
- [02-create-notice-scaffold.sh](scripts/02-create-notice-scaffold.sh) creates
  `<destination>.license-notice.md` using the
  [notice template](assets/third_party_license_notice.md.tera). Pass `--refresh`
  to intentionally replace an existing scaffold after updating its resource.
- [03-append-notice.sh](scripts/03-append-notice.sh) appends a completed
  automated-verification notice to the repository notice file. Pass `--replace`
  to replace that resource's existing aggregate record after a refresh.

Run scripts from any directory. Each resolves the repository root with Git.

## Prerequisites

The scripts require `bash`, `curl`, `git`, `sha256sum`, and `tera` (provided by
the `tera-cli` package). The repository development and CI image provides these
tools. Outside that image, install `tera-cli` as documented in
`CONTRIBUTING.md` and ensure its `tera` command is on `PATH`.

## Verification boundary

The scaffold can identify an SPDX header but cannot determine the complete
license obligations for a file. Keep detection distinct from the automated
verification result, and preserve the exact notice text required by the primary
license or notice source. Automated verification does not represent legal
review or approval by the repository owner or maintainers.
