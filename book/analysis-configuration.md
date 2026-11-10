# SQL Analysis Configuration

Project lint configuration loading is currently available. Deprecated-reference
diagnostics and `expect` evaluation are not yet implemented; see the follow-up
issues below.

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
file; absolute paths are used as supplied. An omitted path, a missing file,
or an empty file means no lint definitions. Whitespace and comments alone
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
Resolution of unqualified names using the selected `search_path` belongs to the
semantic application stage tracked in
[#1086](https://github.com/kazuk/sql-fun/issues/1086); configuration loading preserves the
name components without resolving them.

Configuration loading preserves repeated entries and supported optional fields
without checking target existence or semantic field combinations. Unknown
fields are ignored so fields added or removed across versions do not themselves
prevent SQL analysis. Malformed TOML, known field values that cannot be decoded,
and file read errors other than a missing file are reported with the lint path.

This step provides configuration loading only. Deprecated-reference diagnostics,
semantic validation, and end-of-analysis reporting for `expect = true` are tracked
in [#1086](https://github.com/kazuk/sql-fun/issues/1086),
[#1087](https://github.com/kazuk/sql-fun/issues/1087), and
[#1088](https://github.com/kazuk/sql-fun/issues/1088).
