# Expressions and shared semantic rules

Use these pages for rules shared by statements and clauses:

- [Names and column references](expressions/names.md)
- [Type resolution and casts](expressions/types-and-casts.md)
- [Parameter inference and explicit hints](expressions/parameters.md)
- [Operators and predicates](expressions/operators-and-predicates.md)
- [Function calls](expressions/function-calls.md)
- [CASE, COALESCE, GREATEST and LEAST](expressions/conditional.md)
- [Arrays and rows](expressions/arrays-and-rows.md)
- [COLLATE](expressions/collate.md)

A SQL construct can be legal for its database while its analysis is partial or
unsupported. Each page distinguishes the selected dialect's analyzed behavior
from its support boundaries and links diagnostics actually reported on those
paths. Result type and nullability are static metadata; analysis does not
execute expressions or choose a runtime value.

## SQL/JSON and SQL/XML support boundary

SQL/JSON and SQL/XML scalar constructors, queries and related document
expressions can be retained as syntax, but their semantic expression analysis
is not implemented
([#489](https://github.com/kazuk/sql-fun/issues/489)). They do not yet provide
resolved operand bindings, result types, nullability or parameter relationships.
Current scalar analysis rejects these document expressions rather than
analyzing them as ordinary function calls. Ordinary catalog functions that
accept JSON or XML values have their own function-call analysis; that does not
establish support for these special SQL forms.

Complete analysis of these SQL/JSON and SQL/XML forms remains unfinished
([#864](https://github.com/kazuk/sql-fun/issues/864),
[#865](https://github.com/kazuk/sql-fun/issues/865)); syntax preservation or
round-trip rendering is not evidence of a resolved document expression.
[FROM](clauses/from.md#document-table-sources) describes the separate JSON_TABLE
and XMLTABLE output-shape limitations.

## SQLite literal limitations

SQLite BLOB literals such as `X'4142'` are not yet supported by expression
analysis ([#497](https://github.com/kazuk/sql-fun/issues/497)) and can terminate
analysis instead of producing a structured diagnostic.

Keyword-valued PRAGMA settings, such as `PRAGMA foreign_keys = ON` and
`PRAGMA journal_mode = DELETE`, also lack value analysis
([#498](https://github.com/kazuk/sql-fun/issues/498)). PRAGMA statements are
separately [unsupported as statements](statements.md#unimplemented-sqlite-and-turso-statement-analysis);
recognizing a keyword value does not make those settings effective in later
analysis. This limitation does not imply that all SQL value expressions or
other supported literals are unavailable.
