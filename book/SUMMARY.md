# `sql-fun` User manual

- [Introduction](./introduction.md)
- [Installation](./installation.md)
- [SQLite project bootstrap](./sqlite-bootstrap.md)
- [SQL Functions](./sql_functions.md)
- [SQL analysis coverage and limitations](./sql-analysis.md)
- [Collecting ordered rows](./sql_fun_collector.md)
- [Reusable CTE](./reusable_cte.md)
- [Database Performance Management](./database-performance-management.md)
- [Design philosophies](./design-philosophies.md)

## SQL re-write features

- [Batch query support](./sql-rewrite/batching.md)

## `sql-fun` command line reference

### Subcommands

- [init](./cli/init.md) sub command
- [Extract PostgreSQL metadata](cli/extract.md)
- [check](./cli/check.md) sub command
- [Analyze SQL files](cli/analyze.md)
- [Test SQL contracts](cli/test.md)
<!-- Empty links mark draft chapters in mdBook. -->
<!-- rumdl-disable MD042 -->
- [api](./cli/api.md)
- [api-help]()
<!-- rumdl-enable MD042 -->

## `sql-fun` server api reference

### methods

- [initialize method](./api/initialize.md)
- [Schema context and check analysis API design](./api/check-analysis-design.md)
- [initialized notification](./api/initialized.md)
- [list-tables method](./api/list-tables.md)
<!-- Empty links mark draft chapters in mdBook. -->
<!-- rumdl-disable MD042 -->
- [describe-table method]()
- [validate-sql method]()
<!-- rumdl-enable MD042 -->
