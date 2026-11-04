# FROM

## Overview

FROM analysis binds explicitly supplied table/view definitions, CTE outputs,
derived queries, joins and supported function sources. Aliases determine visible
names; a relation alias hides its original relation name. See
[names and column references](../expressions/names.md) for qualification,
ambiguity and enclosing-query lookup.

Derived queries provide their analyzed result columns. PostgreSQL LATERAL
derived queries can refer to preceding sources; a non-LATERAL derived query does
not gain those same-level source bindings. Function-source arguments use the
available preceding-source scope. Analysis does not execute the query or its
functions.

### Joins and merged columns

Analysis supports source and condition lookup for INNER/CROSS, LEFT, RIGHT and
FULL joins.
Outer joins can make source columns nullable. ON expressions participate in
parameter analysis, but parameter-free PostgreSQL ON operands share the
[boolean-validation gap](where.md#parameter-free-boolean-validation). USING and
NATURAL joins merge matching columns for an unqualified star; qualified stars
retain each source's columns and its outer-join nullability.

For PostgreSQL FULL JOIN USING and NATURAL FULL JOIN, merged keys appear first,
followed by remaining left and right columns. Each merged key uses the common
type and retains the value present on an unmatched row. SQLite keeps merged keys
in their left-source positions and also retains an unmatched right row's value.
This is result-column analysis; opt-in executable-SQL
[wildcard expansion](../statements/select.md#wildcard-projection-expansion) is a
separate operation.

Named and explicit `VARIADIC` function arguments use the same signature binding
and type inference as [scalar calls](../expressions/function-calls.md#postgresql-named-function-arguments).
Declared scalar, composite and OUT/TABLE results determine visible columns;
parameter-dependent calls retain their output relationships until inference
can establish the types. Argument order remains the order written in SQL.

### ROWS FROM and ordinality

PostgreSQL ROWS FROM combines function outputs in written order. Supported
sources also include SQL value expressions such as CURRENT_DATE and scalar SQL
forms such as COALESCE, GREATEST, LEAST, NULLIF and CAST:

```sql
SELECT r.value, r.today, r.position
FROM ROWS FROM (generate_series(1, 2), CURRENT_DATE)
WITH ORDINALITY AS r(value, today, position);
```

Shorter results are padded with NULL, so combined columns are nullable. The
final ordinality column is a non-null bigint unless an outer join null-extends
it. Column aliases rename outputs by position; references must identify a unique
visible column. For a single scalar element, a source alias supplies its default
column name. Named-composite elements expose their fields.

Aggregate calls and nested set-returning calls in FROM elements produce errors;
aggregates in a scalar subquery keep that subquery's scope.

### Record-returning functions

For an unnamed record result, explicitly declare output names and types:

```sql
SELECT result.row_name, result.value
FROM public.crosstab('SELECT 1, 2, 3')
AS result(row_name text, value integer);
```

The function, column types and any extension objects must exist in the selected
schema/builtin metadata. Analysis uses those declarations for qualified and
unqualified references. It does not execute the function or analyze SQL in
string arguments.

Within ROWS FROM, put typed definitions beside each record-returning function.
An outer name-only alias list renames the combined output positionally; omitted
aliases keep their declared names. WITH ORDINALITY uses inner typed definitions
and an outer name-only list, including the final bigint ordinal.

Typed definitions cannot override scalar, named-composite or OUT/TABLE results.
Missing definitions, duplicate names and unresolved type/collation declarations
produce diagnostics. Type modifiers remain separate from type identity:
PostgreSQL builtin character/bit lengths, numeric precision/scale and
time/interval precision are interpreted and validated. Negative numeric scales
and scales above precision are allowed. Time precision above six is retained as
requested and interpreted as six. Extension modifiers are not validated;
analysis does not execute extension typmod input functions.

### TABLESAMPLE

PostgreSQL TABLESAMPLE analysis checks the sampled relation, sampling method,
arguments and optional REPEATABLE seed. Available sampling-method declarations
enable handler, argument count/type and repeatability checks; the seed is
checked separately as a float8-compatible expression. Analysis rejects
prohibited aggregates, windows and set-returning expressions at the sampling
clause's query level without treating all scalar subqueries as forbidden. It
does not validate runtime percentage ranges or sample rows.

Ordinary CLI analysis currently lacks sampling-method declarations, including
for system/bernoulli. It reports unavailable metadata rather than asserting
those valid PostgreSQL methods do not exist. The library can analyze sampling
methods when the caller explicitly supplies their declarations; this does not
establish ordinary CLI support.

## Dialect and engine-specific behavior

ROWS FROM, PostgreSQL record-definition lists and TABLESAMPLE behavior above do
not establish equivalent SQLite analysis. SQLite derived queries, nonrecursive
CTEs and join result/nullability analysis require the `sqlite` feature. SQLite
names use ASCII case-insensitive matching in implemented name-resolution paths;
general expression/type-affinity and complete SQL-legality analysis remain
incomplete. See the [overview](../../sql-analysis.md) for parser boundaries and
CLI/server availability.

### Document table sources

JSON_TABLE source analysis is not implemented
([#491](https://github.com/kazuk/sql-fun/issues/491)). Retaining its paths,
columns and nested syntax does not produce a resolved FROM source; current
analysis can fail before output columns are available.

XMLTABLE analysis currently processes only its row expression through the
function-source path. The XML document/PASSING input, namespaces, declared
output columns, paths, defaults and ordinality are not semantically analyzed
([#490](https://github.com/kazuk/sql-fun/issues/490)). Consequently, this path
does not establish the declared XMLTABLE output shape. These limitations are
separate from ordinary record-returning functions.

### SQLite source limitations

SQLite table-valued function binding and output-column construction remain
unimplemented and can terminate analysis
([#506](https://github.com/kazuk/sql-fun/issues/506)). PostgreSQL
function-source support above does not supply SQLite function/module signatures.

For an aliased parenthesized SQLite FULL JOIN using USING or NATURAL, merged
column analysis incorrectly selects the left input and can report incorrect
type/nullability ([#317](https://github.com/kazuk/sql-fun/issues/317)). For
example, in `SELECT q.id FROM (a FULL JOIN b USING (id)) AS q`, unmatched rows
can supply `id` from either table, including different SQLite storage types.
The general merged-column behavior described above does not establish correct
metadata for this grouped-source case.

## Diagnostics

Names can emit [duplicate_alias](../../diagnostics/duplicate_alias.md),
[ambiguous_column](../../diagnostics/ambiguous_column.md),
[column_source_ambiguous](../../diagnostics/column_source_ambiguous.md), and
[source/column lookup diagnostics](../expressions/names.md#diagnostics).
Function-source diagnostics include:

| Code | Trigger |
| --- | --- |
| [table_function_not_found](../../diagnostics/table_function_not_found.md) | No applicable table-function declaration. |
| [table_function_invalid_expression](../../diagnostics/table_function_invalid_expression.md) | Aggregate or nested set-returning expression in a prohibited FROM-function position. |
| [table_function_definition_required](../../diagnostics/table_function_definition_required.md) | An unnamed record needs typed output definitions. |
| [table_function_definition_not_allowed](../../diagnostics/table_function_definition_not_allowed.md) | Definitions would override an already declared result shape. |
| [table_function_definition_placement](../../diagnostics/table_function_definition_placement.md) | Typed definitions occur at an invalid ROWS FROM/ordinality position. |
| [table_function_duplicate_column](../../diagnostics/table_function_duplicate_column.md) | Duplicate declared output names. |
| [table_function_type_not_found](../../diagnostics/table_function_type_not_found.md) | An output declaration's type cannot be resolved. |
| [table_function_invalid_typmod](../../diagnostics/table_function_invalid_typmod.md) | Invalid builtin output type modifiers. |
| [table_function_too_many_aliases](../../diagnostics/table_function_too_many_aliases.md) | More aliases than available output columns. |
| [parameter_type_mismatch_for_table_function](../../diagnostics/parameter_type_mismatch_for_table_function.md) | Supplied arguments cannot satisfy the table-function call. |

Explicit declaration collations can emit
[collation_not_found](../../diagnostics/collation_not_found.md),
[collation_name_ambiguous](../../diagnostics/collation_name_ambiguous.md),
[collate_operand_type](../../diagnostics/collate_operand_type.md) and
[collate_capability_undefined](../../diagnostics/collate_capability_undefined.md).
A missing type takes precedence over collation validation; failed collation
lookup does not prove the type is non-collatable. Query output metadata can
remain available after declaration validation fails.

TABLESAMPLE diagnostics are:

| Code | Trigger |
| --- | --- |
| [tablesample_metadata_unavailable](../../diagnostics/tablesample_metadata_unavailable.md) | Sampling-method declarations are unavailable; current ordinary CLI limitation. |
| [tablesample_method_not_found](../../diagnostics/tablesample_method_not_found.md) | No matching method is declared. |
| [tablesample_method_ambiguous](../../diagnostics/tablesample_method_ambiguous.md) | More than one matching method is declared. |
| [tablesample_invalid_handler](../../diagnostics/tablesample_invalid_handler.md) | The declared handler is invalid. |
| [tablesample_relation](../../diagnostics/tablesample_relation.md) | Resolved source is not an eligible sampled relation. |
| [tablesample_expression](../../diagnostics/tablesample_expression.md) | Prohibited aggregate/window/set-returning expression at sampling scope. |
| [tablesample_arity](../../diagnostics/tablesample_arity.md) | Argument count differs from verified sampling inputs. |
| [tablesample_argument_type](../../diagnostics/tablesample_argument_type.md) | Known argument type is incompatible with its verified target. |
| [tablesample_repeatable](../../diagnostics/tablesample_repeatable.md) | Verified method does not support REPEATABLE. |
| [tablesample_seed_type](../../diagnostics/tablesample_seed_type.md) | Known seed type cannot satisfy the float8-compatible requirement. |

Ordinary argument expressions also emit their own function/operator and
parameter diagnostics; unavailable metadata does not establish invalid SQL.
