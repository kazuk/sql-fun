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
