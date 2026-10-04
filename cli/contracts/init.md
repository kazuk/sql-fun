# Initialization contracts

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
$ sql-fun init --yes --sql-fun-home "$TMPDIR/init-home" --engine-versions 16 2>&1
* (glob+)
Error: unexpected version 16
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
