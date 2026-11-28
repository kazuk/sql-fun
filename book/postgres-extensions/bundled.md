# Bundled extensions

These references cover the extension scripts shipped with the PostgreSQL 17.6,
18.1, and 19 snapshot catalogs. Each reference lists the versions checked for
offline initialization and describes what the example can verify. The examples
use `public` on the datasource search path and the bundle's default installation
schema for the selected extension. See
[PostgreSQL Extensions](index.md) for selection and schema loading.

| PostgreSQL catalog | Bundled defaults checked | Defaults that initialize |
| --- | ---: | ---: |
| 17.6 | 45 | 45 |
| 18.1 | 46 | 46 |
| 19 snapshot | 47 | 46 |

The PostgreSQL 19 snapshot's default [postgres_fdw](bundled/postgres_fdw.md)
version, `1.3`, fails during initialization. Explicitly selecting `1.2` on
that catalog initializes successfully. See the extension's page for details.

## Extension references

- [amcheck](bundled/amcheck.md)
- [autoinc](bundled/autoinc.md)
- [bloom](bundled/bloom.md)
- [btree_gin](bundled/btree_gin.md)
- [btree_gist](bundled/btree_gist.md)
- [citext](bundled/citext.md)
- [cube](bundled/cube.md)
- [dblink](bundled/dblink.md)
- [dict_int](bundled/dict_int.md)
- [dict_xsyn](bundled/dict_xsyn.md)
- [earthdistance](bundled/earthdistance.md)
- [file_fdw](bundled/file_fdw.md)
- [fuzzystrmatch](bundled/fuzzystrmatch.md)
- [hstore](bundled/hstore.md)
- [insert_username](bundled/insert_username.md)
- [intagg](bundled/intagg.md)
- [intarray](bundled/intarray.md)
- [isn](bundled/isn.md)
- [lo](bundled/lo.md)
- [ltree](bundled/ltree.md)
- [moddatetime](bundled/moddatetime.md)
- [pageinspect](bundled/pageinspect.md)
- [pg_buffercache](bundled/pg_buffercache.md)
- [pg_freespacemap](bundled/pg_freespacemap.md)
- [pg_logicalinspect](bundled/pg_logicalinspect.md)
- [pg_prewarm](bundled/pg_prewarm.md)
- [pg_stash_advice](bundled/pg_stash_advice.md)
- [pg_stat_statements](bundled/pg_stat_statements.md)
- [pg_surgery](bundled/pg_surgery.md)
- [pg_trgm](bundled/pg_trgm.md)
- [pg_visibility](bundled/pg_visibility.md)
- [pg_walinspect](bundled/pg_walinspect.md)
- [pgcrypto](bundled/pgcrypto.md)
- [pgrowlocks](bundled/pgrowlocks.md)
- [pgstattuple](bundled/pgstattuple.md)
- [plpgsql](bundled/plpgsql.md)
- [postgres_fdw](bundled/postgres_fdw.md)
- [refint](bundled/refint.md)
- [seg](bundled/seg.md)
- [sslinfo](bundled/sslinfo.md)
- [tablefunc](bundled/tablefunc.md)
- [tcn](bundled/tcn.md)
- [tsm_system_rows](bundled/tsm_system_rows.md)
- [tsm_system_time](bundled/tsm_system_time.md)
- [unaccent](bundled/unaccent.md)
- [uuid-ossp](bundled/uuid-ossp.md)
- [xml2](bundled/xml2.md)
