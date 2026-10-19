# Type-category oracle samples

These small samples were obtained on 2026-10-19 by running
`cli/src/extract/sql/builtin_types.sql` against the development PostgreSQL
15.19, 16.15, 17.6, and 18.1 servers. Each file records `server_version_num`.
Only representative entries are retained; the full result was checked when
updating the committed 17/18 type metadata. Existing catalog object order and
names were unchanged. Pinned-image CI separately verifies reproducibility of
those committed generations.

The maintained workspace unit tests in
`sem/base_context/test_type_categories.rs` load these samples through the same
record conversion as the production loader, then verify catalog lookup and
serialization. Live servers are not required by the tests.

Small transactional DDL checks on all four servers additionally established:

| Definition | Category |
| --- | --- |
| Enum | `E` |
| Composite and table row | `C` |
| Domain over integer | `N` |
| Domain over integer array | `A` |
| Base type with explicit custom category | `!` |
| Automatically generated arrays of those types | `A` |

The objects were rolled back. The `record` and `_record` samples both have `P`,
so array-like names must not replace catalog category metadata.
