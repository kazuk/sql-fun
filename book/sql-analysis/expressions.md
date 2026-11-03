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
paths. Result type and nullability are static metadata; analysis does not execute
expressions or choose a runtime value.
