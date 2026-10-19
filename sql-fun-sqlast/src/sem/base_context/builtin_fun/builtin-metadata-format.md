# Builtin metadata format

## Current format

Builtin metadata is currently extracted through the CLI using `psql`.
Some structured catalog values are flattened into delimited strings and
reconstructed by the consumer.

This representation is an implementation consequence of the current
extraction pipeline rather than a preferred data model.

## Future format

A future builtin metadata format v2 should use structured values directly.

The v2 producer is expected to be a standalone CLI rather than part of the
main `sql-fun` CLI. This keeps installation-time extraction logic out of the
frequently invoked CLI binary and allows the extractor to use sql-fun's typed
query facilities directly.

Migration to v2 is the intended opportunity to retire the current parsing
utilities rather than incrementally redesigning the v1 representation.

## Function declaration retention

Each overload additionally carries a `signature` object with `returns_set`,
`procedure`, ordered `all_args`, `modes`, `names`, `default_sql` and
`default_start`. Empty mode/name arrays denote PostgreSQL's implicit all-IN or
unnamed declarations; a nonempty array must cover all declaration arguments.
`default_start` counts callable inputs, excluding output-only parameters.
`default_sql` retains the complete server-deparsed expression list, including
commas inside expressions. The consumer associates defaults by list offset and
never splits SQL text heuristically.

The legacy computed result and inference fields remain intact. Complete original
declarations live in per-overload `FunctionSignatureMetadata`, so output-record
synthesis cannot destroy declared cardinality or polymorphic result relationships.
Old payloads without `signature` remain readable for existing semantic consumers;
opt-in loaded-signature reads report unavailable declaration metadata. Regenerate
those catalogs with the updated extractor before using the new API. This additive
format-1 change is implemented and verified with the stacked signature feature;
incomplete extraction metadata is not eligible for integration into `main`.
