---
name: vendor-third-party
description: Vendor a third-party file while recording automated license provenance and appending its required notice. Use for adding or refreshing externally licensed files; do not use for ordinary repository files.
---

# Vendor third-party material

Use this skill when a third-party file must be copied into the repository.

The workflow records collection facts first and completes automated
verification against the primary license and notice material before recording an
attribution. Detected license data is unreviewed input, never a legal
conclusion. Automated verification is not legal review or approval by the
repository owner or maintainers.

## Procedure

1. Choose an immutable source URL and record its revision when one is available.
2. Fetch the resource with [01-fetch.sh](scripts/01-fetch.sh).
3. Create its adjacent OKF scaffold with
   [02-create-notice-scaffold.sh](scripts/02-create-notice-scaffold.sh).
4. Check the source file and the applicable primary license, notice, and
   copyright material. Complete the scaffold, set
   `automated_verification.status: completed`, record the resulting license,
   and put the exact required text between its notice markers.
5. Append the completed notice with
   [03-append-notice.sh](scripts/03-append-notice.sh).
6. Verify the source URL, revision, resource path, and SHA-256 describe the
   same resource. Run the applicable repository checks.

To refresh a resource, fetch it, recreate its scaffold with
`02-create-notice-scaffold.sh --refresh`, complete automated verification, and
replace the aggregate record with `03-append-notice.sh --replace`. The scripts
accept repository-relative destinations only. Do not append a notice until step
4 is complete.

For script arguments, prerequisites, and maintenance notes, read
[README.md](README.md).
