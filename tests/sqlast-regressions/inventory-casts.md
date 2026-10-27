# Schema-context cast migration

`sql-fun-sqlast/tests/schema_context_chain.rs::sql_casts_preserve_array_identity`
was removed after real CLI verification on PostgreSQL 17 and 18. Its five
parameterized cases map to `casts.json` indices 0–4: cast, postgres_cast,
qualified, bounded, scalar. The additional indexed expression asserted inside
each of the first four cases maps to indices 5–8 in the same order. All original
SQL result type/array identities are preserved; the CLI also checks the original
alias `value`, exact column count, absence of diagnostics and nullability.

The other schema-context tests retain layer ordering, publication, catalog keys,
provenance, internal lookup or structured diagnostic assertions; see the broader
retained inventory. They are not superseded by the cast fixtures.

Development oracle verification on PostgreSQL 17 and 18 confirmed `integer[]`
for both ordinary and bounded casts and `integer` after subscripting. Commands
used `psql -X -w -h postgres17`/`postgres18` with `pg_typeof`; no data was loaded.
