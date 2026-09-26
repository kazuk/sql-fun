---
name: vendor-third-party
description: Vendor a third-party file while recording reviewed license provenance and appending its required notice. Use for adding or refreshing externally licensed files; do not use for ordinary repository files.
---

# Vendor third-party material

Use this skill when a third-party file must be copied into the repository.

The workflow records collection facts first and requires a human-quality review
of the primary license and notice material before recording an attribution.
Detected license data is unreviewed input, never a legal conclusion.

## Procedure

1. Choose an immutable source URL and record its revision when one is available.
2. Fetch the resource with [01-fetch.sh](scripts/01-fetch.sh).
3. Create its adjacent OKF scaffold with
   [02-create-notice-scaffold.sh](scripts/02-create-notice-scaffold.sh).
4. Review the source file and the applicable primary license, notice, and
   copyright material. Complete the scaffold, set `review_status: reviewed`,
   and put the exact required text between its notice markers.
5. Append the reviewed notice with
   [03-append-notice.sh](scripts/03-append-notice.sh).
6. Verify the source URL, revision, resource path, and SHA-256 describe the
   same resource. Run the applicable repository checks.

The scripts accept repository-relative destinations only. Do not append a
notice until the review in step 4 is complete.

For script arguments, prerequisites, and maintenance notes, read
[README.md](README.md).
