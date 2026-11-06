# `sql-fun-sqlast`

The SQL parsing and semantic-analysis engine used by
[`sql-fun`](https://crates.io/crates/sql-fun). PostgreSQL is supported; SQLite
support is under development and requires the `sqlite` feature. Parser
acceptance and semantic analysis support differ: see the
[SQL Analysis Reference](https://kazuk.github.io/sql-fun/sql-analysis.html).

For SQL analysis without coupling to the engine's Rust AST and semantic model,
prefer the server's JSON-RPC API, normally accessed through
[`sql-fun-client`](https://crates.io/crates/sql-fun-client). Direct use of this
crate is suitable for deep engine integration, such as custom SQL rewriters or
extensions not exposed by the server, as well as experiments and one-off tools.

The Rust API evolves as database versions, SQL syntax, and analysis features
change. Long-lived direct consumers should expect frequent upgrades and migration
work. Breaking releases follow SemVer, but API stabilization or a 1.0 release is
not a project goal. Contributions upstream may reduce independent maintenance
without freezing the engine API.

See the [Rust API documentation](https://docs.rs/sql-fun-sqlast) for parsing,
mutation, analysis, feature flags, limitations, and examples. The
[installation guide](https://kazuk.github.io/sql-fun/installation.html) describes
native build prerequisites.
