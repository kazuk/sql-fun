# `init` sub command for `sql-fun` CLI utility

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
