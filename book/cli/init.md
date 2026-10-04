# `init` sub command for `sql-fun` CLI utility

The initialization workflow below is planned and is not fully implemented.
To extract usable PostgreSQL catalog and extension artifacts now, use
[`sql-fun extract`](extract.md). The proposed `--docker-image-tag` option below
is not currently accepted by `init`.

`init` sub command prepare and configures `sql-fun`.

1. Create `sql-fun` home directory.
   - `sql-fun` home directory contains
     - DB Engine builtin objects information
       - Extract from `PostgreSQL` docker image
     - DB Engine extensions information
     - SQL Syntax highlighter themes

## Usage

### Interactive setup

```console
sql-fun init
```

### Download release artifact for DB Engine builtin object information

Shorthand

```console
sql-fun init --yes
```

This command line same as bellow.

```console
sql-fun init --yes --github-repo "kazuk/sql-fun" 
```

### Extract DB Engine builtin object information from your database

```console
sql-fun init --yes --database-url postgres://your-db-host/database
```

### Extract DB Engine builtin object information from docker image

```console
sql-fun init --yes --docker-image-tag "postgres:latest"
```

## Options

- `--yes`, `-y` : Disable prompts (assume yes)
- `--sql-fun-home`: Home directory path. (optional)
  - default is `${HOME}/.sql-fun`
- `--database-url`: Extract DB Engine built-in object information from actual
  database instance. (optional)
- `--docker-image-tag`: Extract DB Engine built-in object information from
  container. (optional)
- `--github-repo`: Download source for DB Engine built-in object information.
  (optional)
  - default is `kazuk/sql-fun`
- `--sql-fun-version`: `sql-fun` version to use when downloading artifacts
  (semver)
  - default is latest stable release
- `--engine-version`: Limit initialization to the specified database engine
  versions.
  - default: install ALL engine versions in artifact
- `--postgres-extensions`: Limit initialization to the specified `PostgreSQL`
  extensions
  - default: install ALL extensions

Extension selections use `name` or `name@version`, for example `uuid-ossp` or
`uuid-ossp@1.1`. Version names must be valid UTF-8. An explicit version selects
its installation script. When the
version is omitted, sql-fun uses `default_version` from the extension's primary
control file. A missing script for that default is an error.

If the control file is absent or has no default, sql-fun infers a version only
when exactly one installation script exists. Specify `name@version` when several
installation versions are available. Update scripts (`name--from--to.sql`) are
excluded from inference. Resolution requires a direct installation script
(`name--version.sql`); it does not execute upgrade chains or apply a custom
control-file `directory` setting. Control-file include directives are unsupported
and produce an error.
