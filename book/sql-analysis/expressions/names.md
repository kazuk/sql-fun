# Names and column references

## Overview

PostgreSQL column analysis accepts `column`, `table_or_alias.column`,
`schema.table.column`, and `database.schema.table.column`. References bind only
to relations visible in the current query or an enclosing query. A table or view
alias hides its original relation name, even when the alias repeats the table
name: `FROM public.items AS items` allows `items.id`, but hides
`public.items.id`.

Distinct unaliased relations with the same local name can coexist across
schemas: `SELECT a.t.id, b.t.id FROM a.t, b.t` binds each column to its named
relation. The short qualifier `t.id` (and `t.*` ) is ambiguous in that query,
even if only one relation contains the requested column. An unqualified column
can still resolve when exactly one relation supplies it. `SELECT *` includes
both sources in FROM order. Repeating the same relation without distinct aliases
remains invalid, as does reusing an explicit alias.

In SELECT targets, `*`, `alias.*`, and `schema.table.*` expand visible source
columns in order. PostgreSQL also expands `(composite_expression).*`, including
named composite values, table rows, and `ROW(...)` values. A composite column
can be expanded as `column.*` when no source uses that name. A row expansion
outside a projection target, or on a scalar value, produces a diagnostic. When a
composite input's type is still unresolved, the result retains an explicit
pending expansion until parameter inference determines its fields.

### Composite fields

PostgreSQL member access follows domains over composite types and table row
types, including nested domains. For example, `(value::address_domain).city`
returns the declared type of `city`. If that field is itself a domain, its
domain identity is preserved. A missing field and member access on a
non-composite base (such as an integer or an array) produce distinct
diagnostics.

Merged USING/NATURAL join columns and qualified-star nullability are described
under [FROM](../clauses/from.md). Executable-SQL wildcard expansion is a
separate explicit [SELECT option](../statements/select.md).

## Dialect and engine-specific behavior

To analyze PostgreSQL database-qualified columns, explicitly configure the
actual `database-name` in the selected datasource's
[installation metadata](../../installation.md).

`database-name` is the exact logical database identifier, without SQL quoting;
it must be nonempty and contain no NUL character. It is independent of the
metadata datasource name (the configured datasource key), and is never inferred
from a connection or a query. If a database-qualified column is used without
this setting, analysis retains a `column_database_name_required` failure. A
different database name produces `column_cross_database`; PostgreSQL
cross-database column references are unsupported.

With the `sqlite` feature, SQLite uses `database.table_or_alias.column`, where
`database` is an explicitly referenced SQLite namespace such as `main`.
PostgreSQL's four-component column form is invalid in SQLite.

SQLite table/view qualification and CTE relation-name lookup use ASCII
case-insensitive matching. Derived-query and CTE output-column lookup still
requires matching spelling, as does window-name binding. Those incomplete
output-column and window paths do not make differently cased identifiers
invalid SQLite SQL. PostgreSQL and SQLite qualification rules do not themselves
extend support for schema- or database-qualified star expansion.

## Diagnostics

- [ambiguous_column]: more than one visible binding supplies an unqualified
  column.
- [column_source_ambiguous]: a source qualifier identifies more than one source.
- [column_source_not_found]: a qualified source is not visible.
- [column_qualification_invalid]: the qualification shape is not valid for the
selected dialect.
- [column_database_name_required]: a PostgreSQL database-qualified reference
lacks explicit database-name metadata.
- [column_cross_database]: a PostgreSQL column requests a different database.
- [column_not_found_in_table]: the resolved table has no requested column.
- [column_not_found_in_view]: the resolved view has no requested column.
- [column_not_found_in_cte]: the selected CTE has no requested column.
- [column_not_found_in_sub_query]: the selected derived query has no requested
output column.
- [column_not_found_in_table_function]: the selected table function has no
requested column.
- [column_not_found_in_source]: the selected source has no requested column.
- [duplicate_alias]: a source name or explicit alias is repeated where
uniqueness is required.
- [composite_type_field_not_found]: a resolved composite type has no requested
  field.
- [member_access_requires_composite_type]: member access uses a known
non-composite value.
- [query_invalid_star_projection_placement]: a row expansion is used outside a
projection target.
- [query_star_projection_requires_composite_type]: star expansion uses a scalar
  value.

[ambiguous_column]: ../../diagnostics/ambiguous_column.md
[column_source_ambiguous]: ../../diagnostics/column_source_ambiguous.md
[column_source_not_found]: ../../diagnostics/column_source_not_found.md
[column_qualification_invalid]: ../../diagnostics/column_qualification_invalid.md
[column_database_name_required]: ../../diagnostics/column_database_name_required.md
[column_cross_database]: ../../diagnostics/column_cross_database.md
[column_not_found_in_table]: ../../diagnostics/column_not_found_in_table.md
[column_not_found_in_view]: ../../diagnostics/column_not_found_in_view.md
[column_not_found_in_cte]: ../../diagnostics/column_not_found_in_cte.md
[column_not_found_in_sub_query]: ../../diagnostics/column_not_found_in_sub_query.md
[column_not_found_in_table_function]: ../../diagnostics/column_not_found_in_table_function.md
[column_not_found_in_source]: ../../diagnostics/column_not_found_in_source.md
[duplicate_alias]: ../../diagnostics/duplicate_alias.md
[composite_type_field_not_found]: ../../diagnostics/composite_type_field_not_found.md
[member_access_requires_composite_type]: ../../diagnostics/member_access_requires_composite_type.md
[query_invalid_star_projection_placement]: ../../diagnostics/query_invalid_star_projection_placement.md
[query_star_projection_requires_composite_type]: ../../diagnostics/query_star_projection_requires_composite_type.md
