# PostgreSQL syntax generation (maintainers)

The workspace uses `pg_query` 18.1.0, whose libpg_query sources embed PostgreSQL
18.6. `pg_query.proto` must match the proto packaged in that exact crate release.
Its immutable source, checksum and license attribution are recorded in
[`pg_query.proto.license-notice.md`](pg_query.proto.license-notice.md).

From the repository root, the existing `./sql-fun-sqlast/generate` command builds
`schema.json` from the proto, generates the Rust wrappers with xbuild, and formats
`src/syn/generated.rs`. Neither generated file should be edited by hand.

Generation requires HTTPS access to GitHub releases at execution time. The script
downloads Buf v1.75.0 into a temporary directory, checks its pinned SHA256 before
execution, and cleans up on success or failure. It supports the Linux x86_64 and
Linux aarch64 builder platforms. It requires the existing Bash, curl, coreutils,
Rust/Cargo and rustfmt development tools; no installed Buf, container runtime or
Docker socket is needed. Ordinary Cargo builds use the committed generated files
and do not run this download.

The script runs Buf in `sql-fun-sqlast/`, using the existing module configuration,
then runs the existing `cargo x enum-types` and `cargo x struct-types` generators.
It stages both outputs until generation and formatting succeed. A second
invocation with the same inputs must leave their content unchanged.

xbuild excludes `SummaryResult`, which belongs to libpg_query's separate summary
API and contains nested API result messages, from the syntax wrappers. It retains
all PostgreSQL parser node variants. Raw parse trees and PostgreSQL's analyzed
internal trees are different contracts: adding an internal protobuf enum value
does not introduce a new SQL grammar form.
