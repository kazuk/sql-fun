# CASE and COALESCE expression inventory

The [schema-dependent follow-up](inventory-schema.md) supersedes this earlier
builtin-only migration's custom-schema exclusions. Use its detailed mappings
for the current destinations and retained internal assertions.

These rows map semantic-only source cases to public SQL regression fixtures.
Expected diagnostics and result-column properties are checked by the runner;
parameter domains and semantic AST details remain checked by the Rust tests.

| Source test group | Source scenarios | Fixture cases | Rust assertions retained |
| --- | --- | --- | --- |
| `sem/parameter_resolution/case_tests.rs::failures_retain_case_and_allow_next_statement` | `CASE WHEN 1 THEN $1 ELSE 1 END` (`non_boolean`); `CASE WHEN $1 THEN 1 ELSE true END` (`mixed_results`); `CASE WHEN $1 THEN $1 ELSE 1 END` (`contradictory`) | `case_semantic.json`: observed CLI parameter-conflict and unresolved-parameter diagnostics plus result-column nullability/type absence | Input assignment rejection, retained CASE arm count, and successful next-statement analysis. |
| `sem/parameter_resolution/case_tests.rs::searched_conditions_require_boolean` | Searched CASE parameter, multiple arms, and repeated predicate | Covered separately by the assigned `case_parameters.rs` integration work | Retain boolean-only parameter domain and AST result metadata in the synthetic-context tests. |
| `sem/parameter_resolution/case_tests.rs::null_test_does_not_choose_an_input_type` | `$1 IS NULL` predicate does not constrain the parameter | No fixture: public CLI reports `parameter_type_unresolved`, unlike the lightweight unit context | Unrestricted parameter domain. |
| `sem/parameter_resolution/case_tests.rs::simple_case_keeps_the_resolved_comparison` | Parameter in CASE selector or WHEN operand; typed result parameter | Assigned `case_parameters.rs` group covers real equality overloads | Comparison binding and correlated admissible assignments. |
| `sem/scalar_expr/coalesce/tests.rs::incompatible_concrete_operands_retain_a_failed_type_domain` | `COALESCE(1,true)` | `coalesce_semantic.json`: unreachable argument warning and unknown common result type | Empty admissible type domain and retained arguments. |
| `sem/scalar_expr/coalesce/tests.rs::failures_retain_operands_and_allow_next_statement` | `COALESCE($1,1) WHERE $1` (`contradictory`); `COALESCE(1,$1,true)` (`unreachable_mixed`) | `coalesce_semantic.json`: CLI `parameter_constraint`, `unreachable_coalesce_argument`, and `parameter_type_unresolved` diagnostics plus result-column metadata | Retain empty admissible type domain, COALESCE arguments, and successful next-statement analysis. |
| Same group | `COALESCE($1,1,true)` (`mixed`) | No fixture: builtin-only CLI reports only `parameter_type_unresolved`; it omits the source test's required `parameter_constraint` | Keep the source constraint-domain and AST assertions; this case is not migrated. PostgreSQL 17 and 18 reject it with “COALESCE types integer and boolean cannot be matched.” |
| `sem/scalar_expr/coalesce/tests.rs::an_unreachable_parameter_still_constrains_the_result` | `COALESCE(1,$1)` | Assigned `coalesce_parameters.rs` integration work includes the expression with real catalog data | Retain input-domain restriction and warning severity/render metadata; the CLI diagnostics do not duplicate these internal assertions. |
| `sem/scalar_expr/coalesce/tests.rs::untyped_operands_retain_the_semantic_expression` | Single, multiple, or NULL-adjacent untyped parameters | No CLI equivalent: the assertion is parameter presence and semantic-node retention without a concrete public result | Entire assertion remains Rust-only. |
| `sem/scalar_expr/coalesce/tests.rs` SQLite cases | Heterogeneous arguments, one-argument arity, SQLite case folding | Excluded from this PostgreSQL 17/18 fixture runner | Retain SQLite-specific behavior in Rust. |

The fixture files are `case_semantic.json` and `coalesce_semantic.json`. Their
manifest rows are:

```text
case_semantic<TAB>builtin<TAB>-<TAB>3<TAB>17,18
coalesce_semantic<TAB>builtin<TAB>-<TAB>3<TAB>17,18
```

The focused runner used a temporary suite copy and report directory. Both
fixtures passed on PostgreSQL 17 and 18. The unresolved-parameter diagnostic
is emitted by query metadata when a parameter has no single inferred type but
its constraint domain is nonempty or unrestricted; an empty domain retains its
`parameter_constraint` diagnostic. Thus unresolved parameter typing does not
establish that PostgreSQL accepts the full expression. Oracle `PREPARE` checks
on PostgreSQL 17 and 18 reject the mixed CASE/COALESCE expressions for their
incompatible result types, reject the non-boolean CASE/WHERE conditions, and
accept `COALESCE(1,$1)`. The `COALESCE(1,$1)` case in the
assigned real-catalog integration group remains in Rust: the builtin-only CLI
reports `unreachable_coalesce_argument` and `parameter_type_unresolved`, while
the source test's contract is its internal input-domain restriction and warning
metadata. This is not a duplicate public assertion. PR #761 resolved the earlier
catalog initialization issue (#727); default/project schema levels remain outside
the accepted migration scope.
