# Common AST SQL emission

`ast::Statement::deparse_sql(dialect)` is the output boundary for an owned,
unresolved, possibly edited statement. It lives in `sql-fun-sqlast`; catalog
resolution and server integration are separate operations. The dialect must
agree with the recorded parsing dialect. Output is deterministic SQL without a
trailing semicolon; ordinary formatting and comments are outside the fidelity
contract. SQLite type spelling and trivia affecting declared types or casting
affinity are retained as structured owned syntax.

## Representation and emission decisions

Use the existing common AST rather than introducing a second public syntax model.
The private `SqlEmit` implementations belong to their owning types. DDL and
SQL/JSON/XML implementations are descendants of their owning modules so retaining
private fields does not require exposing internal syntax. The shared writer owns
quoting, the output buffer, lists, dialect checks, and structured errors. It never
uses source slices, retained scan tokens, or native ASTs to generate SQL.

Explicit parentheses protect operator association and compound-query grouping.
SQLite compound terms cannot acquire independently ordered/limited or nested
right-hand terms by flattening them. Unsupported edited shapes return errors.
Grouping-clause implicit row lists use `Grouping::Simple`; an explicit ROW
expression remains a single grouping expression. Treating both as ROW would
change the grouping keys.

The existing PostgreSQL native deparser and Turso formatter are validation
references, not production fallbacks. Native PostgreSQL formatting requires its
own protobuf AST and has assertion/panic cases. Turso formatting of reconstructed
binary expressions does not establish the required parentheses. Direct emission
avoids two reverse AST mappings and lets every rejection remain fallible.

Syntax retention additions remain private to the consuming syntax representation.
In particular, composite fields keep common `TypeName` modifiers and collations,
then derive the existing semantic `CreateCompositeType` when resolving. Semantic
and serialized shared data models remain intact. PostgreSQL ALTER actions retain
owned common operation/payload types instead of a descriptive native tag string.
Function options and SQL bodies retain their own structured representations;
quoted procedural bodies stay text literals.

## SQLite frontend contract

The locked `turso_parser` 0.8.1 AST normalizes type names and drops comments and
bracket delimiters. Stock SQLite includes these spellings in declared types and
uses them when determining affinity. Recovering them from the normalized native
name cannot preserve behavior, and adding consumer-specific fields to the native
AST would change a shared representation.

Common statement parsing therefore uses a private adaptation of the pinned
upstream parser and lexer. Immutable sources and their verified MIT provenance
are recorded under `src/sqlite_parser/upstream` and in `THIRD_PARTY_NOTICES.md`.
The build script applies checked single-match adaptations into Cargo's output
directory; the generated modules reuse the dependency's existing AST, token and
error types. Both parser and macro dependencies are pinned to 0.8.1. This private
component is packaged in sqlast, so published builds do not rely on a workspace
Cargo patch. The existing semantic parser remains unchanged.

The temporary adaptation and source-span retention are gated by both `sqlite`
and the default-enabled `turso-type-syntax-workaround` feature. The workaround
feature does not imply SQLite. With it disabled, common parsing uses
`::turso_parser::parser::Parser` directly, and the build script returns before
reading or transforming copied sources. Native-AST conversion and SQL emission
remain the same; only lossless source information is unavailable. Features unify
across Cargo dependents, so a single dependent enabling the workaround selects
it for all consumers of this crate.

The reason is the confirmed upstream compatibility bug
[tursodatabase/turso#9791](https://github.com/tursodatabase/turso/issues/9791),
tracked internally in [#432](https://github.com/kazuk/sql-fun/issues/432).
Remove the workaround only after a published parser API retains SQLite-compatible
type semantics or exposes sufficient lossless grammar-produced source information.
Evaluate the published crate version against the maintained declared-type,
storage-affinity and CAST regressions, then record the version/results and removal
decision in #432. A runtime fix without the required parser API is insufficient.
Until then, the opt-out path is an upstream evaluation configuration, not an
equivalent correctness path. Keep correctness tests unchanged on the default path.
The standard workspace validation also compiles isolated sqlast configurations
without default features (with no extra features, `sqlite`, and `turso_parser`)
and the workaround without SQLite, so workspace feature unification cannot hide
opt-out compilation failures.

The adaptation captures exact grammar-produced type boundaries, including numeric
modifiers, without changing the native type model. A type-local retention pass
pairs these events with converted common nodes in normalized grammar order,
including comma LIMIT and grouped UPDATE assignments. It validates names,
statement boundaries and complete consumption. These are exact parsing events,
not guessed token-derived diagnostic locations. Source ranges are consumed during
parsing; emission uses only decoded owned common-AST parts, numeric values and
validated trivia. Array/inline-struct parser extensions remain rejected for stock
SQLite. Maintained runtime tests cover declared types and casts across nested
queries, multiple statements and DML clauses.

## Coverage ownership and maintained checks

This matrix describes emission coverage for successful common-AST conversions;
it does not extend the public parser to every statement in either frontend.
Exhaustive dispatch keeps future common variants visible to the compiler.

| Family | Retained syntax and regression boundary |
| --- | --- |
| Names, literals, types, parameters | Qualification, quoting/escaping, literal kinds, modifiers, arrays, interval/time-zone syntax; stored numbered slots, including repeats and gaps. SQLite type names, quote delimiters, numeric modifiers and internal trivia retain declared types and casting affinity. |
| Expressions | All `Expr` variants, operator/predicate forms, calls and special SQL call syntax, CASE, subqueries, grouping, indirection, session values, CURRENT OF and MERGE_ACTION. SQLite-only parser extensions are rejected when they have no stock SQLite grammar. |
| Queries | SELECT/VALUES/set operations; targets, DISTINCT, FROM/join grouping and aliases, typed functions, sampling, windows/frames, grouping, ordering/limits, locks and SELECT INTO. |
| WITH | Ordered CTE bodies and declared columns, materialization, RECURSIVE, SEARCH/CYCLE; complete newly inserted and original subtrees. |
| DML | INSERT source/targets/overriding/conflicts, UPDATE assignments including grouped multi-assignment, DELETE USING, WITH and RETURNING; reachable MERGE clauses and actions. |
| Tables and constraints | Persistence, inheritance, typed/partitioned tables and bounds, access method/storage parameters, commit/tablespace clauses; column collation/compression/storage, defaults/generated/identity, keys, FK actions, exclusion elements and index/validation/deferrability attributes. |
| Other DDL | View/check options, extension options, SET/RESET kinds/local scope, enums/sequences, DROP, comment/owner targets and signatures, composite/domain/function/procedure/schema/index/CTAS/materialized view, and raw PostgreSQL ALTER actions. |
| SQL/JSON and XML | Scalar/constructor/predicate/aggregate forms and options; JSON_TABLE nesting/paths/behaviors, XML expressions and XMLTABLE namespaces/columns. |

Tests are maintained through the normal workspace test steps. Public integration
tests exercise CTE insertion on SELECT/INSERT/UPDATE/DELETE in both dialects after
the input buffers have been dropped. Independent PostgreSQL frontend comparisons
ignore source locations, not meaningful fields. Native canonical SQL comparisons
cover additional expression/query/DDL cases; implicit ordinary row constructors
may normalize to explicit ROW without changing their elements. SQLite tests also
prepare/execute emitted SQL with bundled `rusqlite`, checking stored parameter
slots, observable results, declared types, and casting affinity. Deep-tree tests
use the repository's isolated small-stack harness within frontend decoding limits.

Negative tests assert `DeparseError` classification, node context and unchanged
input. No partial SQL is returned. Unknown internal/native-only tags rejected by
conversion are distinct from retained legal syntax and from dialect-invalid
edited trees. Add a named semantic or frontend regression whenever a retention
field or emission rule changes; a common-AST-only round trip cannot detect a field
already lost by both conversions.

The public error type and rendering method are documented in `src/lib.md`. Users
need behavior, binding slots, feature requirements and limitations; this internal
matrix belongs here rather than in the CLI manual. No new runtime executable or
CI image dependency is introduced. Bundled SQLite is a test-only library and uses
the existing C compiler prerequisites.
