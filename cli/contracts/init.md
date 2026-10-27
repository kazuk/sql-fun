# Initialization contracts

The shared setup executes every CLI invocation with an unavailable runtime-tool
PATH. This suite covers help, argument parsing and preflight only; it must not
provision Docker or contact runtime services. Actual extraction and initialization
run in `.local/bin/test-postgres-extraction`, not this default contract directory.

## Setup

```scrut
$ mkdir "$TMPDIR/init-contract" && cd "$TMPDIR/init-contract"
```

## Help requires no runtime tools

```scrut
$ PATH="$TMPDIR/no-tools" sql-fun init --help > /dev/null
```

## Unsupported extraction targets fail explicitly

```scrut
$ sql-fun init -y -H "$TMPDIR/init-home" --engines postgres --docker \
> --engine-versions 16 2>&1
Error: PostgreSQL 16 is not supported for builtin extraction; * (glob)
[1]
```

## Sources conflict without exposing connection values

```scrut
$ sql-fun init --docker --database-url 'postgres://secret@localhost/postgres' 2>&1
error: the argument '--docker' cannot be used with '--database-url <DATABASE_URL>'

Usage: * (glob)

For more information, try '--help'.
[2]
```
