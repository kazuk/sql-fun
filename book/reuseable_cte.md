# Reuseable Query

`sql-fun` enables query reuse via CTE.

## Reuseable query

Reuseable query written in bellow.

- CTE Catalog
- SQL Function

## CTE Catalog

Multiple CTE catalog file in CTE catalog directory are combined the CTE Catalog.

CTE catalog file must have extension `json`, `toml`, `yaml`.

CTE catalog file must have key `sql_fun_cte_catalog_format_version` and `sql_dialect`, See [CTE Catalog file format](.#cte_catalog_file_format).

All CTE catalog file combind in single CTE catalog, you can reuse CTE catalog entry in CTE files.

### CTE Catalog file format

CTE catalog file is a JSON like file format. JSON, TOML, YAML are valid CTE catalog file.

Can be mix a JSON, TOML, YAML in CTE catalog.

```json
{
  "sql_fun_cte_catalog_format_version": "1",
  "sql_dialect": "PostgreSQL",
  "cte": {
    "select_active_users": {
      "query": "select u.id, u.name from users u where u.deleted_at is not null"
    },
    "select_incomplete_orders": {
      "query": "select o.* from orders o where o.deliver_at is not null"
    }
  }
}
```

Same as written in TOML.

```toml
sql_fun_cte_catalog_format_version= "1"
sql_dialect= "PostgreSQL"

[cte.select_active_users]
query="""
select u.id, u.name from users u where u.deleted_at is not null
"""

[cte.select_incomplete_orders]
query="""
select o.* from orders o where o.deliver_at is not null
"""

```

### Field specfications

- `sql_fun_cte_catalog_format_version` string : valid value is "1".
- `sql_dialect` string: valid value is "PostgreSQL" or "SQLite".
  - PostgreSQL can accept DML as a CTE, then `insert`, `update`, `delete` are valid CTE catalog entry.
  - SQLite can not accept DML as a CTE, then only `select` are valid CTE catlog entry.
- `datasource` optional string: CTE catalog activate in specified database. 
  - When ommited: CTE catalog activate in default datasource.
- `cte` hash: CTE name to CTE object hash
  - `query` string: `sql-fun` s SQL text. Place-holders like a `${name}`.
  - `recursive` optional bool: Require `with recurcive` to use this CTE catalog entry.
  - `with` optinal hash string to string: reuse other query from CTE catalogs.
    - key is a CTE name. value is a alias name using in query. See [Nested CTE catalog expansion](.#nested_cte_catalog_expansion)
  - `renames`: optional hash string to rename target.
    - key is a new name. value is array of rename target place holder.

### Nested CTE catalog expansion

When CTE catalog entry defined with `with`: Reuse other CTE catalog entry.

```toml
[cte.select_active_users]
query="""
select u.id, u.name from users u where u.deleted_at is not null
"""

[cte.select_prior_users]
with = { 
    # using select_active_users CTE catalog entry with alias name active_users
    select_active_users="active_users"
}
query="""
select au.id, au.name from active_users au where au.prior_flag=1
"""
```

When using select_prior_users, final query has a CTE "active_users".

```sql
with 
  active_users as (select u.id, u.name from users u where u.deleted_at is not null),
  prior_users as (select u.id, u.name from active_users au where au.prior_flag=1)
select ...
  from prior_users pu
```

When conflict alias, `sql-fun` automaticaly rename alias with `_NN` postfix and resolve conflicts.

### Renameing named place holder in CTE

CTE query can containes `sql-fun` named place holder like `${name}` syntax.

Named place holder shown in final query in default. `renames` override this behavior.

Example: rename `id` place holder in select_user_by_id with `user_id`.

```toml
[cte.select_active_user_by_id]
query="""
select u.* 
from users 
where u.id=${id} 
  and u.deleted_at is not null
"""

[cte.select_user_order]
query="""
select o.* 
from active_user u 
  inner join orders o 
    on u.id = o.user_id
"""
with = {
   select_active_user_by_id="active_user"
}
renames = {
   user_id = [ { cte="select_active_user_by_id", place_holder="id" } ]
}
```

when renames has multiple place-holder target, all target place holder renamed in one place holder name.

`renames` must not conflict in all CTE entry in CTE expansion.

### Positional place holder in CTE

`sql-fun` policy : Engine accepts SQL, that is valid SQL. not block to that.

You can write positional place holder in CTE, Not arranged in `sql-fun`.

Not to recommends use it. Makes breaking changes when CTE entry edited.

## CTE catalog expansion in CLI

You can use a CTE catalog in with `sql-fun` CLI subcommand `cte expand`.

Example: Using CTE catalog entry `select_active_users`

```console
$ sql-fun cte expand \
  --with select_active_users active_users \
  `select id, name from active_users`
with active_users as (select u.id, u.name from users u where u.deleted_at is not null)
select id, name from active_users
```
