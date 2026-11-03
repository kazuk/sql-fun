# SQL Analysis Reference

Use this reference to find how sql-fun analyzes your SQL: names and scopes,
parameter and result types, nullability, schema effects, and diagnostics. It
describes current analysis support and limitations, not a guarantee that all SQL
accepted by the parser is fully analyzed or executable.

- [Top-level statements](sql-analysis/statements.md): queries, row changes,
  and schema definitions or alterations.
- [Common clauses](sql-analysis/clauses.md): shared query scopes and clauses.
- [Expressions and shared semantic rules](sql-analysis/expressions.md): names,
  casts, parameters, operators, calls, rows, arrays, and collations.
- [Diagnostic reference](diagnostics/): look up an emitted diagnostic code.

[Installation](installation.md) owns datasource configuration, catalog inputs,
search paths, database identity, and known roles. Choose those inputs
explicitly; analysis does not discover a database schema or execute SQL to fill
gaps. [CLI analyze](cli/analyze.md) and the [analysis
API](api/check-analysis-design.md) describe how to request analysis and
interpret its output.

## Analysis boundaries

The reference distinguishes SQL legality, parser acceptance, semantic analysis,
and schema application. Valid SQL may remain unsupported. Known names or result
columns can remain available after a diagnostic; that does not establish that
all analysis completed successfully. A rejected schema statement can report a
problem without changing the schema used by subsequent analysis.

PostgreSQL analysis uses the selected catalog and explicitly configured engine
version. Parsing PostgreSQL 18 grammar does not establish PostgreSQL 18 semantic
support: implemented version checks can report
[incompatible_engine_version](diagnostics/incompatible_engine_version.md).
Unimplemented conversions or analysis operations can still terminate analysis.
Examples include unsupported utility statements, renamed RETURNING row images,
INSERT/UPDATE destination row images, temporal constraints and unfinished
constraint enforcement/inheritance operations. See the applicable statement
pages. Deferrable unique-key catalog definitions and recursive CTE binding
remain unsupported; expressions unsupported in SELECT do not become supported
merely because another statement contains them. Result-column star analysis
covers tables, ordinary views, derived queries, nonrecursive CTEs and joins; the
[optional executable-SQL rewrite](sql-analysis/statements/select.md) has its own
narrower expansion policy.

SQLite library analysis support requires the `sqlite` feature and is partial. Do
not infer CLI/server availability from library analysis support: the [schema
initialization API](api/check-analysis-design.md) currently supports PostgreSQL.
Each reference page identifies dialect differences and the relevant support
boundary. Successful binding alone does not prove complete grouping, window or
other runtime legality validation.

## Existing reference links

The following entry points preserve links to the earlier coverage page. Their
linked reference pages own the current descriptions; obsolete explanations have
been corrected against the implementation and maintained tests.

<!-- Explicit legacy anchors keep the sidebar focused on the new syntax tree. -->
<!-- rumdl-disable MD033 -->
<a id="postgresql-row-constructors"></a>

**PostgreSQL row constructors**.
[Reference](sql-analysis/expressions/arrays-and-rows.md). Row construction,
field projection and dialect-specific row-value rules.

<a id="postgresql-range-predicates"></a>

**PostgreSQL range predicates**.
[Reference](sql-analysis/expressions/operators-and-predicates.md). BETWEEN
bounds, comparisons, type constraints and NULL behavior.

<a id="postgresql-symmetric-range-predicates"></a>

**PostgreSQL symmetric range predicates**.
[Reference](sql-analysis/expressions/operators-and-predicates.md). Symmetric
bounds and comparison requirements.

<a id="wildcard-projection-expansion"></a>

**Wildcard projection expansion**.
[Reference](sql-analysis/statements/select.md). Opt-in PostgreSQL executable-SQL
star expansion and its schema-drift limits.

<a id="conflicting-schema-definitions"></a>

**Conflicting schema definitions**.
[Reference](sql-analysis/statements.md#conflicting-schema-definitions). Object
namespaces, duplicate definitions and rejected schema changes.

<a id="postgresql-alter-column-attributes"></a>

**PostgreSQL ALTER COLUMN attributes**.
[Reference](sql-analysis/statements/alter-table.md). Column attributes, version
checks, operation phases and catalog effects.

<a id="postgresql-rows-from-sources"></a>

**PostgreSQL ROWS FROM sources**.
[Reference](sql-analysis/clauses/from.md). Source composition, aliases,
ordinality and nullability.

<a id="postgresql-record-returning-table-functions"></a>

**PostgreSQL record-returning table functions**.
[Reference](sql-analysis/clauses/from.md). Typed output definitions, type
modifiers and placement constraints.

<a id="sqlite-parser-limitations"></a>

**SQLite parser limitations**.
The SQLite parser limits expression depth to 100, including long operator chains
as well as nested parentheses. There is no sql-fun setting to increase this
limit. UPDATE and DELETE with ORDER BY or LIMIT are rejected by the inherited
parser; enabling SQLite's `SQLITE_ENABLE_UPDATE_DELETE_LIMIT` build option does
not enable those clauses in sql-fun. There is no sql-fun setting or
preprocessing workaround for these forms.

The parser recognizes Turso extensions too. Parser acceptance does not establish
that stock SQLite or `rusqlite` can execute those extensions.

<a id="sqlite-analysis-coverage"></a>

**SQLite analysis coverage**.
[Reference](sql-analysis/statements/select.md). SQLite SELECT, derived queries,
CTEs, joins and partial clause/type checking. See also
[names](sql-analysis/expressions/names.md),
[parameters](sql-analysis/expressions/parameters.md) and
[Reference](sql-analysis/expressions/conditional.md).

<a id="insert-analysis"></a>

**INSERT analysis**.
[Reference](sql-analysis/statements/insert.md). Destination/source checks,
defaults, conflicts and result metadata. Shared [view
writability](sql-analysis/statements/create-view.md) and
[RETURNING](sql-analysis/clauses/returning.md) have separate owners.

<a id="create-rule-definitions"></a>

**CREATE RULE definitions**.
[Reference](sql-analysis/statements/rules.md). Rule definition scopes, actions,
view effects and DROP RULE.

<a id="create-trigger-definitions"></a>

**CREATE TRIGGER definitions**.
[Reference](sql-analysis/statements/triggers.md). Trigger definitions, dialect
differences, catalog effects and DROP TRIGGER.

<a id="postgresql-truth-predicates"></a>

**PostgreSQL truth predicates**.
[Reference](sql-analysis/expressions/operators-and-predicates.md). IS
TRUE/FALSE/UNKNOWN and their operand and result rules.

<a id="postgresql-greatest-and-least"></a>

**PostgreSQL GREATEST and LEAST**.
[Reference](sql-analysis/expressions/conditional.md). Common result types,
ordering requirements and nullability.

<a id="postgresql-array-constructors"></a>

**PostgreSQL ARRAY constructors**.
[Reference](sql-analysis/expressions/arrays-and-rows.md). List and query
constructors, nesting, coercion and empty arrays.

<a id="parameter-analysis"></a>

**Parameter analysis**.
[Reference](sql-analysis/expressions/parameters.md). Explicit hints, admissible
types and application input type requirements.

<a id="null-safe-distinctness-predicates"></a>

**Null-safe distinctness predicates**.
[Reference](sql-analysis/expressions/operators-and-predicates.md). Scalar/row
comparisons and null-safe results.

<a id="delete-analysis"></a>

**DELETE analysis**.
[Reference](sql-analysis/statements/delete.md). Target scope, USING, cursor
support, writable views and result metadata.

<a id="merge-analysis"></a>

**MERGE analysis**.
[Reference](sql-analysis/statements/merge.md). Version-dependent actions, scope,
RETURNING and writable targets.

<a id="postgresql-alter-index-coverage"></a>

**PostgreSQL ALTER INDEX coverage**.
[Reference](sql-analysis/statements/alter-index.md). Supported index
alterations, statistics and atomicity.

<a id="qualified-column-references"></a>

**Qualified column references**.
[Reference](sql-analysis/expressions/names.md). Qualification, aliases,
ambiguity, correlation and database identity.

<a id="table-renames-in-schema-files"></a>

**Table renames in schema files**.
[Reference](sql-analysis/statements/alter-table.md). Table identity, dependency
propagation and rename limitations.

<a id="column-renames-in-schema-files"></a>

**Column renames in schema files**.
[Reference](sql-analysis/statements/alter-table.md). Column identity, dependency
propagation and dialect-specific rename behavior.

<a id="project-role-inventory"></a>

**Project role inventory**.
[Reference](sql-analysis/statements/alter-owner.md). Explicit known roles,
symbolic session owners and offline ownership analysis.

<a id="explicit-collate-annotations"></a>

**Explicit COLLATE annotations**.
[Reference](sql-analysis/expressions/collate.md). Expression lookup, type
capability checks and dialect-specific support.

<a id="membership-predicates"></a>

**Membership predicates**.
[Reference](sql-analysis/expressions/operators-and-predicates.md). IN
lists/subqueries, row widths, parameter constraints and NULL behavior.

<a id="pattern-predicates"></a>

**Pattern predicates**.
[Reference](sql-analysis/expressions/operators-and-predicates.md). Pattern
operators, ESCAPE, dialect-specific coercion and limitations.

<!-- rumdl-enable MD033 -->
