# sql-fun – SQLs into first-class functions

`sql-fun` turns SQL queries and statements into **first-class functions**.

It assumes that you **can write SQL and want to write SQL**.

It doesn't try to hide SQL behind abstractions or pretend it knows better than your database.
It just makes **SQL fit cleanly into your development workflow** — statically validated, safe, and enjoyable.

If you're looking for an ORM: this isn't it.  
If you're writing raw SQL and want your tools to help more: this is it.

---

✨ For Rust developers, `sql-fun` provides the **best developer experience** by integrating directly with proc-macros to generate strongly typed async functions at compile time.  
For other ecosystems, `sql-fun` can be used as a **stand-alone SQL semantic validator** and linter, and may serve as a backend for language-specific code generators (Go, Python, TypeScript, …).


## What is `sql-fun`

`sql-fun` is a toolchain for executing and validating SQL queries and statements.

- It is designed for **PostgreSQL**:
  - SQL is parsed using `pg_query`, a full PostgreSQL parser.
    - You can use the full range of PostgreSQL syntax with no limitations.
  - All SQL statements are validated statically:
    - SQL syntax validation
    - Schema-aware validation (optional)
- It provides a command-line tool for schema inspection and query analysis:
  - Explore tables, columns, views, etc.
    - Describe objects in your schema without a live database connection.
  - Find queries referencing a specific table or column in your application
    - More reliable than plain text grep.
  - Run SQL syntax and schema-aware checks on standalone SQL files.
- It does not try to model your entire database schema.
  - Instead, it focuses only on what your application actually *does*—the SQL queries it runs.
  - This keeps the tool lightweight, focused, and reliable.
- For Rust developers, `sql-fun` integrates with proc-macros:
  - Generates strongly typed functions at **compile time**.
  - Requires **no runtime dependencies** in your application.
  - Compatible with `tokio-postgres`, the primary runtime `sql-fun` targets.

## Directory structure

- `src` :
  Procedual macro implementations

- `sql-fun-sqlast/` :
  `sql-fun` semantic SQL parser 

- `xbuild/` : (private crate)
  Project internal utility

- `cli/` :
  CLI user utility for `sql-fun`.

- `examples/` :
  various example.

- `book/` :
  User Manual

- `sql-fun-server/` :
  `sql-fun` Backend JSON RPC Server.

- `sql-fun-client/` :
  Client crate for `sql-fun-server`

- `parse_dat/` : (private crate)
  PostgreSQL internal `.dat` file parser CLI tool.

- `postgres/` : (Git submodule)
  PostgreSQL server source code from mirror.

- `docs` :
  Internal documentations for `sql-fun`


## License

`sql-fun` is primarily distributed under the terms of the MIT license, with portions covered by various BSD-like licenses.


## Small Contributions Are Welcome

Even small tools, shell scripts, or one-off experiments are welcome.

For example:

- 🐚 A shell script that processes `sql-fun.develop.toml` to extract table names
- 📋 A `jq` snippet to list all query parameter names
- 📝 A Gist that generates basic test stubs for each query

If it's useful to you, it might be useful to others.  
You can share it in a PR, an issue, or just drop a link in a discussion thread.
