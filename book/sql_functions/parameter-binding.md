# Parameter binding

- In the SQL string, bind parameters using `${...}` syntax.
- Bindings become `$1`, `$2`, … for PostgreSQL and `?1`, `?2`, … for SQLite.
  Repeated names reuse a binding; ordering follows first occurrence in the
  expanded SQL. Text inside SQL comments and string literals is not a binding.

## Binding from a DTO

Mark one function argument with `#[parameter_source]` to supply parameters that
have no same-named normal function argument:

```rust
#[sql_fun::sql_statement(
    sql = "insert into users (id, name) values (${id}, ${name})"
)]
async fn insert_user(
    client: &tokio_postgres::Client,
    #[parameter_source] dto: &UserDto,
) -> Result<u64, tokio_postgres::Error> {}
```

The same parameter-source settings also work with synchronous `rusqlite`
functions when the `sqlite` feature is enabled.

Here, `UserDto` provides `id()` and `name()` getters. A normal argument named
`id` would take precedence over `dto.id()`.

| Source attribute | Fallback for `${name}` |
| --- | --- |
| `#[parameter_source]` | `source.name()` |
| `#[parameter_source(getter_prefix = "get_")]` | `source.get_name()` |
| `#[parameter_source(field_bind = true)]` | `source.name` |
| `#[parameter_source(field_bind = false)]` | `source.name()` |

Keyword-shaped bind names map to valid Rust members: `${type}` uses
`source.r#type()` (or `source.r#type` with field binding). Only the four names
`self`, `Self`, `super`, and `crate` receive a trailing underscore (`_`) for
both getter and field binding: `self_`, `Self_`, `super_`, and `crate_`.
For example, `${self}` uses `source.self_()` or `source.self_`, respectively.
Other names do not receive this suffix. For getter prefixes, this mapping
is applied after concatenation: `get_` plus `self` calls `source.get_self()`.
Normal direct arguments retain their original Rust identifiers.

`field_bind = false` is the default and can be combined with `getter_prefix`.
`field_bind = true` and `getter_prefix` are mutually exclusive, even when the
prefix is empty. Initially, only one parameter source is supported per query
function. Invalid attribute settings are reported by the macro.

The generated bindings borrow these expressions. Rust checks member existence,
visibility, types, and borrowing. If neither a normal argument nor a source is
available, the generated function uses the ordinary `&name` reference; rustc reports
an undefined identifier if necessary.

For conditional SQL, a source remains available whenever its argument remains
in the generated child function. Match-arm introduced normal arguments take
precedence over the source. `#[cond]` and `#[parameter_source]` have separate
semantics and may coexist: if a fragment consumes the source argument, that
source no longer exists in its child functions. A match expression such as
`#[sql_fragment(choice, dto.mode(), (), { /* arms */ })]` can inspect the DTO
while preserving it for child-function bindings.
