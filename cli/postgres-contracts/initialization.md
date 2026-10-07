# PostgreSQL initialization installed-CLI contract

This contract runs against an installed `sql-fun` executable with Docker and
PostgreSQL client tools available. The version list must track the stable
initialization targets in `PostgresVersionInventory`.

## Setup

```scrut
$ mkdir "$TMPDIR/postgres-initialization-contract" && cd "$TMPDIR/postgres-initialization-contract"
```

## Initialize every supported PostgreSQL version

```scrut
$ sql-fun init --yes --engines postgres --docker \
> --sql-fun-home "$TMPDIR/postgres-initialization-contract/home" \
> --engine-versions 17,18 \
> --docker-host-address "$SQL_FUN_TEST_DOCKER_HOST" \
> --docker-bind-address "$SQL_FUN_TEST_DOCKER_BIND" \
> > "$TMPDIR/postgres-initialization-contract/init.log" 2>&1
```

## Report each completed installation

```scrut
$ home="$TMPDIR/postgres-initialization-contract/home"
> init_log="$TMPDIR/postgres-initialization-contract/init.log"
> grep -Fx "PostgreSQL 17: initialized at $home/postgres/17" "$init_log" && \
> grep -Fx "PostgreSQL 18: initialized at $home/postgres/18" "$init_log"
PostgreSQL 17: initialized at * (glob)
PostgreSQL 18: initialized at * (glob)
[0]
```

## Install usable builtin metadata

```scrut
$ home="$TMPDIR/postgres-initialization-contract/home"
> for version in 17 18; do
>   for artifact in metadata.json schema.sql pg_type.json functions.json \
>     func_detail.json binary_op.json left_unary_op.json \
>     right_unary_op.json casts.json; do
>     test -s "$home/postgres/$version/$artifact"
>   done
> done
[0]
```
