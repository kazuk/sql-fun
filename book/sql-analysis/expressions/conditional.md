# CASE, COALESCE, GREATEST and LEAST

## Overview

PostgreSQL parameter-dependent CASE expressions constrain searched WHEN
conditions to boolean and simple CASE operands through equality overloads.
Result branches use implicit common-type conversion, considering ELSE before
THEN branches in source order. Omitted ELSE retains a nullable NULL result.
Input and result domains can contain several alternatives; inspect them through
`ParameterConstraints` rather than assuming a single result type. For example,
`CASE WHEN true THEN $1 ELSE 1 END` accepts integer and bigint inputs with
corresponding numeric result alternatives.

PostgreSQL COALESCE uses the same common-type constraints, considering operands
in source order. All operands participate, even after a known non-null operand;
unreachable operands produce a warning. The analyzed expression retains its
operands and determined implicit conversions. For example, `COALESCE($1,1)`
admits integer and bigint inputs with corresponding result alternatives; inspect
`ParameterConstraints::result_column` when no single result is known. COALESCE
nullability metadata remains conservative and does not yet prove that a non-null
fallback makes the result non-null.

When every CASE result branch depends on unresolved parameters, the result
domain can remain open. This also applies to ARRAY and GREATEST/LEAST.
`ParameterConstraints::allows` checks explicit input assignments against the
retained constraints; it does not supply analysis hints. All-unknown SQL
literals, such as bare NULLs or untyped strings, instead select `text` as their
common type: CASE and GREATEST/LEAST return `text`, while an ARRAY of those
literals has type `text[]`.

### GREATEST and LEAST

`GREATEST` and `LEAST` analyze every operand and report a PostgreSQL common
result type, implicit coercions, and nullability. NULL operands are ignored; a
guaranteed non-null operand makes the result non-null. All-unknown literal
inputs resolve to `text`. Incompatible types and types without a comparison
implementation, such as `json`, produce diagnostics.

Numbered parameters participate in common-type analysis, including nested
expressions, subqueries, and nonrecursive CTEs. Several admissible input types
can produce different result types; the analysis retains these alternatives.
When every result operand depends on unresolved parameters, the result domain
can remain open. `ParameterConstraints::allows` can check explicit input
assignments against the retained constraints. For example,
`GREATEST($1,NULL)` admits explicitly typed integer, text, and other ordered
inputs, while an all-unknown literal expression resolves to `text`. An explicit
SQL cast can specify an operand's type. These expressions are separate from
aggregate `MIN` /`MAX` and SQLite scalar `min` /`max`.

Mixed-type selection uses PostgreSQL type categories and the preferred-type
flags in builtin cast metadata. If analysis reports missing type or comparison
facts, refresh the selected builtin catalog using the existing
[extraction workflow](../../cli/extract.md).

## Dialect and engine-specific behavior

The common-type and equality constraints above describe PostgreSQL. SQLite CASE
can report a common type when result branches already have matching types, but
does not use PostgreSQL boolean or implicit-cast constraints.
Parameter-dependent SQLite result typing remains incomplete. SQLite COALESCE
retains its operands without PostgreSQL common-type conversions;
heterogeneous/parameter-dependent result typing remains incomplete and its
inputs remain unconstrained by COALESCE. SQLite COALESCE requires at least two
arguments. PostgreSQL GREATEST/LEAST are separate from aggregate MIN/MAX and
SQLite scalar min/max.

## Diagnostics

- [incompatible_case_values]: established CASE results have incompatible common
  types.
- [coalesce_expr_type_unknown]: established COALESCE inputs have no common
result type.
- [unreachable_coalesce_argument]: an earlier operand is known non-null; this is
a warning, and later operands are still analyzed.
- [min_max_empty]: GREATEST/LEAST has no operands.
- [min_max_incompatible_types]: GREATEST/LEAST inputs have no compatible common
  type.
- [min_max_missing_type_facts]: common-type selection lacks required type facts.
- [min_max_not_ordered]: the selected type has no required ordering
  implementation.
- [min_max_missing_comparison_facts]: the selected type lacks required
comparison facts.

[incompatible_case_values]: ../../diagnostics/incompatible_case_values.md
[coalesce_expr_type_unknown]: ../../diagnostics/coalesce_expr_type_unknown.md
[unreachable_coalesce_argument]: ../../diagnostics/unreachable_coalesce_argument.md
[min_max_empty]: ../../diagnostics/min_max_empty.md
[min_max_incompatible_types]: ../../diagnostics/min_max_incompatible_types.md
[min_max_missing_type_facts]: ../../diagnostics/min_max_missing_type_facts.md
[min_max_not_ordered]: ../../diagnostics/min_max_not_ordered.md
[min_max_missing_comparison_facts]: ../../diagnostics/min_max_missing_comparison_facts.md

Unsatisfiable parameter relationships also use
[parameter diagnostics](parameters.md#diagnostics); too few SQLite COALESCE
arguments report `parameter_constraint`.
