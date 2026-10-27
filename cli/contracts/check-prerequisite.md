# `check` prerequisite smoke contract

## Setup

```scrut
$ mkdir "$TMPDIR/cli-contract-fixture" && \
> cd "$TMPDIR/cli-contract-fixture" && \
> test "$PWD" = "$TMPDIR/cli-contract-fixture"
```

## Help does not require `cargo-expand`

```scrut
$ PATH="$TMPDIR/without-cargo-expand" sql-fun --help > /dev/null && \
> PATH="$TMPDIR/without-cargo-expand" sql-fun check --help > /dev/null
```

## `check` reports a missing `cargo-expand`

```scrut
$ PATH="$TMPDIR/without-cargo-expand" sql-fun check 2>&1
Error: *requires cargo-expand*install --locked cargo-expand* (glob)
[1]
```
