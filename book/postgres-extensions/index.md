# PostgreSQL Extensions

Select extensions in your PostgreSQL datasource or with the
[CLI extension selection options](../cli/index.md). For example, add
`extensions = [{ name = "hstore" }]` to your datasource. This selects hstore
at the default version declared by its control file for the chosen PostgreSQL
catalog. To pin a version, supply the `version` field or use `name@version`
on the CLI. If a selected extension has no declared default, supply a version
from the options reported by the CLI.

sql-fun reads the selected extension's installation script and any updates
needed to reach the selected version. It also loads prerequisites declared by
the control files before the scripts that need them. See
[analyze schema loading](../cli/analyze.md#schema-loading) for dependency
ordering, script requirements, and installation-owner configuration. If a script
refers to named owners or grantees, [configure those PostgreSQL
roles](../installation.md#configure-known-postgresql-roles) in the datasource.
Each bundled reference lists the roles that its script requires.

Use `analyze --schema-level extensions` to check SQL against the builtin
catalog, bundled schema, and selected extensions without loading a project
schema dump. Successful initialization makes interpreted declarations available
for static analysis. PostgreSQL installs the extension and runs its functions,
procedural code, and other runtime operations. A successful analysis does not
verify that those operations will succeed on a server.

See [Bundled extensions](bundled.md) for versions verified with the
shipped catalogs and the analysis limits for each extension.

To supply an extension yourself, follow
[Configure a non-bundled PostgreSQL extension](external.md).
