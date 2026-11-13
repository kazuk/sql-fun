# SQL Analysis Configuration

Project lint declarations can be interpreted against a loaded project schema
and applied to application queries. CLI configuration transport and
end-of-analysis expectation evaluation remain tracked in the follow-up issues
below.

To select a project lint configuration file, add `lint_configuration` to the
selected datasource table:

```toml
[package.metadata.database.default]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
lint_configuration = "lint.toml"
```

Relative lint paths are resolved from the directory containing the metadata
file; absolute paths are used as supplied. An omitted path or an existing empty
file means no lint definitions. An explicitly configured path that does not exist
returns a filesystem I/O error (`NotFound`). Whitespace and comments alone
also mean no definitions. No lint file is discovered automatically.

The lint file uses `[[lint.deprecated]]` entries:

```toml
[[lint.deprecated]]
kind = "table"
name = ["public", "users"]
column = "legacy_name"
expect = true

[[lint.deprecated]]
kind = "function"
name = ["legacy_function"]

[[lint.deprecated]]
kind = "enum_label"
name = ["public", "mood"]
label = "sad"
```

Supported target kinds are `table`, `view`, `function`, `type`, `domain`, and
`enum_label`. For tables and views, `column` designates one column; omitting it
identifies the whole object. An enum-label entry names its enum type and uses
`label` for the exact label text. Function targets apply by name to all overloads;
no argument-type list is required.

Each `name` element is an exact identifier value. Preserve case and place names
containing dots in a single element; do not include SQL identifier quotes.
`name = ["users"]` is unqualified, while `name = ["public", "users"]` is qualified.
Unqualified names resolve using the query analysis catalog and selected
`search_path`. Configuration loading itself preserves the name components
without resolving them.

Configuration loading preserves repeated entries and supported optional fields
without checking target existence or semantic field combinations. Unknown
fields are ignored so fields added or removed across versions do not themselves
prevent SQL analysis. Malformed TOML, known field values that cannot be decoded,
and all file read errors, including missing files, are reported with the lint path.

When project linting is applied, references to configured objects produce a
`deprecated` warning. Resolved references count through aliases, `*` expansion,
CTEs, and nested queries. Function targets cover all overloads of the resolved
name. A column entry applies only to that column; an enum-label entry applies
only to that label. Unavailable source positions are omitted.

An unknown kind, an inapplicable field, or an unresolved object, column, or label
produces a `lint_configuration` warning and disables that entry. Other entries
continue to apply. A missing column never becomes a whole-table rule, and a
missing label never becomes a whole-type rule. Duplicate entries are detected
after name resolution and produce `duplicate_lint_target` warnings. They form
one effective target with `expect = true` if any duplicate requests it.

`expect = true` requires a corresponding deprecated-reference warning during the
analysis period. Configuration warnings do not meet that expectation;
unresolved entries retain their expectation. Period accumulation and final
expectation reporting are tracked in
[#1087](https://github.com/kazuk/sql-fun/issues/1087). CLI configuration transport
and end-to-end invocation are tracked in
[#1088](https://github.com/kazuk/sql-fun/issues/1088).

Enum-label references are not yet distinguished by semantic analysis, so valid
`enum_label` entries currently produce no deprecated-reference warnings. This
limitation is tracked in [#1084](https://github.com/kazuk/sql-fun/issues/1084).
