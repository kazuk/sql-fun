# pgvector offline analysis validation

This record for [#1293] tests extension initialization and three SQL expressions
through an installed distribution. It is evidence for the external-extension
guide in [#1277].

## Inputs

- sql-fun CLI and server 0.2.0, packaged from commit
  `9159d1fda054c34954ea58333450ed761b473558` for
  `x86_64-unknown-linux-gnu` with
  `.local/bin/check-test-flow --step=packaging`.
- PostgreSQL 18.3 builtin catalog (server version number `180003`), extracted
  from a database without pgvector on the comparison instance into
  `<sql-fun-home>/postgres/18/`.
- [pgvector v0.8.7][pgvector] at commit
  `f37c13f68b57d2c3472b2214fbcff699d6d34876`. The upstream
  `vector.control` declares `default_version = '0.8.7'`.

## Unpack the distribution and obtain pgvector

Start in a temporary directory outside the sql-fun source checkout. Set
`archive` to the absolute path of the matching Linux distribution tarball.

```sh
mkdir bin
tar -xzf "$archive" -C bin
unset SQL_FUN_SERVER_COMMAND
export SQL_FUN_SERVER="$PWD/bin/sql-fun-server"
bin/sql-fun --version
bin/sql-fun-server --version

git clone --depth 1 --branch v0.8.7 https://github.com/pgvector/pgvector.git
git -C pgvector rev-parse HEAD
```

## PostgreSQL comparison

The live comparison used a disposable PostgreSQL 18.3 instance with the
module built from the same pinned pgvector source. With the
[PostgreSQL source archive][postgresql-source] unpacked beside `pgvector`,
use GCC, Make, Bison and Flex to build a scratch installation.

```sh
(
  cd postgresql-18.3
  ./configure --prefix="$(dirname "$PWD")/pg" --without-icu --without-readline --without-zlib
  make -j8
  make install
)
make -C pgvector PG_CONFIG="$PWD/pg/bin/pg_config" OPTFLAGS=
make -C pgvector PG_CONFIG="$PWD/pg/bin/pg_config" install
mkdir -p socket
pg/bin/initdb -D "$PWD/pgdata" --auth=trust --no-locale --encoding=UTF8
pg/bin/pg_ctl -D "$PWD/pgdata" -l "$PWD/postgres.log" \
  -o "-c listen_addresses='' -k $PWD/socket -p 55432" -w start
pg/bin/psql -X -v ON_ERROR_STOP=1 \
  "host=$PWD/socket port=55432 dbname=postgres user=$(id -un)"
```

In `psql`, create the extension and describe the same queries.

```sql
CREATE EXTENSION vector VERSION '0.8.7';
SELECT extversion FROM pg_extension WHERE extname = 'vector';
SELECT '[1,2,3]'::vector AS embedding
\gdesc
SELECT vector_dims('[1,2,3]'::vector) AS dimensions
\gdesc
SELECT '[1,2,3]'::vector <-> '[4,5,6]'::vector AS distance
\gdesc
```

## Initialize the builtin catalog and stage extension SQL

Use an existing PostgreSQL 18 catalog installation or initialize a scratch
home from the comparison instance before going offline. The connection string
uses a private local Unix socket and a separate catalog database.
Extracting from a database with pgvector already installed includes its base
types in `pg_type.json` and causes conflicting type definitions during
offline initialization ([#1294]).

```sh
connection="host=$PWD/socket port=55432 user=$(id -un)"
pg/bin/psql -X -v ON_ERROR_STOP=1 "$connection dbname=postgres" \
  -c 'CREATE DATABASE builtin_catalog TEMPLATE template0;'
PATH="$PWD/pg/bin:$PATH" bin/sql-fun init --yes --install-default-theme \
  --engines postgres --engine-versions 18 --sql-fun-home "$PWD/sql-fun-home" \
  --database-url "$connection dbname=builtin_catalog"
```

Stage the control file, the versioned installation script, and all upstream
update scripts in the loader's PostgreSQL extension directory. The versioned
installation script is a byte-for-byte copy of `sql/vector.sql`, as in the
[upstream Makefile][pgvector-makefile]. Copying it directly requires only
Git and standard shell utilities.

```sh
extension_dir="$PWD/sql-fun-home/postgres/18/extensions"
mkdir -p "$extension_dir"
install -m 0644 pgvector/vector.control pgvector/sql/vector--*--*.sql "$extension_dir/"
install -m 0644 pgvector/sql/vector.sql "$extension_dir/vector--0.8.7.sql"
```

The install script is `vector--0.8.7.sql`. The update script manifest is
the `sql/vector--*--*.sql` glob in the pinned source tree (41 files). All
files are installed flat beside `vector.control`. The validation staged
the versioned file emitted by `make -C pgvector sql/vector--0.8.7.sql
PG_CONFIG="$PWD/pg/bin/pg_config"` and checked its bytes against
`sql/vector.sql` with `cmp`.

Save the following datasource as `Cargo.toml` in the scratch directory.

```toml
[package]
name = "pgvector-offline-example"
version = "0.1.0"

[package.metadata.database.vector_example]
engine = "PostgreSQL"
dialect = "postgres"
version = "18"
connector = "tokio-postgres"
extensions = [{ name = "vector" }]
```

## Run offline analysis

Stop the comparison instance with
`pg/bin/pg_ctl -D "$PWD/pgdata" -m fast -w stop`. Run each offline probe in a
separate invocation. Select a fresh sql-fun server
namespace with `USER` to avoid reusing a server from a different installation.

```sh
export USER=pgvector-clean

analyze() {
  bin/sql-fun --sql-fun-home "$PWD/sql-fun-home" \
    --metadata-file "$PWD/Cargo.toml" --datasource vector_example \
    analyze --schema-level extensions --sql-file "$1"
}

printf "%s\n" "SELECT '[1,2,3]'::vector AS embedding;" > embedding.sql
printf "%s\n" "SELECT vector_dims('[1,2,3]'::vector) AS dimensions;" > dimensions.sql
printf "%s\n" "SELECT '[1,2,3]'::vector <-> '[4,5,6]'::vector AS distance;" > distance.sql
analyze embedding.sql
analyze dimensions.sql
analyze distance.sql
```

## Initial results

The CLI and server ran from the extracted archive in the scratch directory.
The PostgreSQL instance was stopped during analysis. A system call trace of
the cast probe confirmed that the packaged server opened
`postgres/18/extensions/vector--0.8.7.sql` and that the CLI connected only
to the local sql-fun server for analysis. Setting `version = "0.8.7"` on the
extension selection produced the same cast result. Leaving `vector.control` in place
and temporarily removing the SQL scripts gave
`No installation path for extension vector@0.8.7` at initialization.

| Probe | Offline exit | Offline result column | PostgreSQL result column |
| --- | --- | --- | --- |
| Cast | 0 | `embedding public.vector` | `embedding vector` |
| Function | 0 | `dimensions pg_catalog.int4` | `dimensions integer` |
| Unqualified distance operator | 1 | `distance`, type unresolved | `distance double precision` |

PostgreSQL recorded `extversion = '0.8.7'`. The offline distance probe
reported `binary_operator_overload_resolution_failed` and
`undefined_operator`. As a diagnostic control,
`SELECT '[1,2,3]'::vector OPERATOR(public.<->) '[4,5,6]'::vector AS distance;`
resolved to `pg_catalog.float8` offline. PostgreSQL has `<->` overloads in
both `pg_catalog` and `public`; the qualified control confirms that
initialization registered the extension operator. This identified the
unqualified lookup gap tracked in [#797].

## Verification after the operator lookup fix

[PR #1297][lookup-fix] changed PostgreSQL operator lookup to consider visible
overloads across schemas. The CLI and server were packaged again with
`.local/bin/check-test-flow --step=packaging` from commit
`4217e971260ef7dfbe787007bec1a160ef3dd1de` for
`x86_64-unknown-linux-gnu`. Both binaries reported version 0.2.0.

The new archive was unpacked in a fresh directory outside the checkout.
The rerun reused the clean PostgreSQL 18.3 catalog, the datasource above,
and the same three SQL files. The staged control file, installation script,
and all 41 update scripts were checked byte-for-byte against the pinned
pgvector source. PostgreSQL was stopped. A trace showed the packaged
server opening `postgres/18/extensions/vector--0.8.7.sql` with the
unversioned selection and the CLI connecting to the local sql-fun server.

| Probe | Offline exit | Offline result column | PostgreSQL result column |
| --- | --- | --- | --- |
| Cast | 0 | `embedding public.vector` | `embedding vector` |
| Function | 0 | `dimensions pg_catalog.int4` | `dimensions integer` |
| Unqualified distance operator | 0 | `distance pg_catalog.float8` | `distance double precision` |

All three probes completed without diagnostics and matched the recorded
PostgreSQL 18.3 column names and types for the same SQL and pgvector release.
This is the working example for [#1277].

[#797]: https://github.com/kazuk/sql-fun/issues/797
[#1294]: https://github.com/kazuk/sql-fun/issues/1294
[#1293]: https://github.com/kazuk/sql-fun/issues/1293
[#1277]: https://github.com/kazuk/sql-fun/issues/1277
[pgvector]: https://github.com/pgvector/pgvector/tree/v0.8.7
[pgvector-makefile]: https://github.com/pgvector/pgvector/blob/v0.8.7/Makefile
[postgresql-source]: https://ftp.postgresql.org/pub/source/v18.3/
[lookup-fix]: https://github.com/kazuk/sql-fun/pull/1297
