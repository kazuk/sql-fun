# Conditional SQL

Write a fragment placeholder as `#[name]` in SQL and define its alternatives
with `#[sql_fragment(...)]`. The macro generates each SQL variant at compile
time; calling the function selects a variant with a Rust match expression.
This works with all four query/statement macros and both connectors.

## Fragment arguments and match arms

The attribute takes four arguments:

1. The fragment name used in `#[name]`.
2. The Rust expression to match.
3. A parenthesized list of function argument names consumed by that match;
   use `()` to preserve all arguments.
4. A Rust-style match block whose arm bodies are SQL string literals.

Use `#[args(name: Type, ...)]` on an arm to introduce typed arguments into that
variant. Pattern bindings alone do not declare their types. Guards are allowed;
Rust checks match exhaustiveness and binding/guard validity. A missing fragment
definition, an unused definition or a non-string arm body is a macro error.

```rust,ignore
#[sql_fun::sql_query_many(sql = "select id, name from users #[where_clause]")]
#[sql_fragment(where_clause, name, (name), {
    #[args(n: &str)]
    Some(n) => "where name=${n}",
    None => ""
})]
async fn find_user<F, Fut>(
    client: &tokio_postgres::Client,
    #[cond] name: Option<&str>,
    mut collector: VecCollector<UserRow>,
    handler: F,
) -> Result<VecCollector<UserRow>, anyhow::Error>
where
    F: Fn(VecCollector<UserRow>, UserRow) -> Fut,
    Fut: std::future::Future<Output = Result<VecCollector<UserRow>, anyhow::Error>>,
{}
```

`Some(n)` selects SQL with a bound `n`; `None` selects SQL without a WHERE clause.
`VecCollector` and its handler are described in [signatures](signatures.md#collector-type).

## Consumed and forwarded arguments

The explicit `(name)` list removes `name` from the variant functions. An arm's
`#[args(...)]` adds its values to the chosen variant's call. For `sql_query_many`
these are placed before the final collector/handler pair. Other preserved
arguments are forwarded. Fragments are expanded recursively in SQL usage order;
a later match can consume arguments introduced by an earlier arm.

`#[cond]` is accepted as a condition marker and removed from emitted Rust
signatures. It does **not** itself remove an argument from variant signatures
or control binding: the explicit consumed list removes the argument. Put its
name there when needed. To inspect a DTO and preserve it for SQL
binding, use an expression with an empty consumed list, for example
`#[sql_fragment(choice, dto.mode(), (), { /* string-valued arms */ })]`.
See [DTO binding](parameter-binding.md#binding-from-a-dto) for precedence.

## Tuple matches

Group related choices in the input type when only certain combinations are
valid. For example, `Option<(i32, i32)>` requires both range bounds together:

```rust,ignore
#[sql_fun::sql_query_many(sql = "select id, name from users #[where_clause]")]
#[sql_fragment(where_clause, (name, age_range), (name, age_range), {
    #[args(n: &str, age_min: i32, age_max: i32)]
    (Some(n), Some((age_min, age_max))) =>
        "where name=${n} and age between ${age_min} and ${age_max}",
    #[args(n: &str)]
    (Some(n), None) => "where name=${n}",
    #[args(age_min: i32, age_max: i32)]
    (None, Some((age_min, age_max))) =>
        "where age between ${age_min} and ${age_max}",
    (None, None) => ""
})]
async fn find_user<F, Fut>(
    client: &tokio_postgres::Client,
    #[cond] name: Option<&str>,
    #[cond] age_range: Option<(i32, i32)>,
    mut collector: VecCollector<UserRow>,
    handler: F,
) -> Result<VecCollector<UserRow>, anyhow::Error>
where
    F: Fn(VecCollector<UserRow>, UserRow) -> Fut,
    Fut: std::future::Future<Output = Result<VecCollector<UserRow>, anyhow::Error>>,
{}
```

`INVALID_QUERY` is not an accepted arm value: every arm requires a string
literal. Validate invalid input combinations in application code before calling
the query, or represent valid combinations with a suitable Rust type.

## Generated query names

Generated child query names combine the original function name and normalized
pattern text: letters become lowercase, non-ASCII-alphanumeric characters become
underscores, and repeated underscores collapse. For example, a `Some(n)` pattern
produces a suffix `some_n_`, and `_` produces the child name `find_user__`.
These are query identifiers used when inspecting generated query metadata and
lockfiles; choose patterns with distinct normalized names. Guard text does not
distinguish child function names.

Conceptually, the first example dispatches as follows:

```rust,ignore
match name {
    Some(n) => find_user_some_n_(client, n, collector, handler).await,
    None => find_user_none(client, collector, handler).await,
}
```

Each selected child executes its own statically expanded SQL; it does not
concatenate arbitrary input text into SQL at runtime.
