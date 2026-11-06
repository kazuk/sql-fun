# `sql-fun extract import-extensions`

Import an administrator-produced extension bundle into a new catalog candidate:

```text
sql-fun extract import-extensions --bundle <BUNDLE> \
  --catalog <CATALOG> --output <OUTPUT>
```

| Option | Behavior |
| --- | --- |
| `--bundle <PATH>` | Required uncompressed tar bundle produced by the export script. |
| `--catalog <PATH>` | Required previously extracted catalog directory, read without modification. |
| `--output <PATH>` | Required new or empty output directory. |
| `-h`, `--help` | Print help. |

```sh
sql-fun extract import-extensions --bundle extensions.tar \
  --catalog ./work/postgres-17 --output ./work/postgres-17-with-extensions
```

The bundle and extracted catalog must match the PostgreSQL release. Success
exits with status 0; validation and I/O failures exit nonzero, and invalid CLI
usage exits with status 2. See [the extraction guide](../extract.md) for bundle
requirements, output safety, and the administrator workflow. Root-level
[common options](../README.md#common-options) precede `extract`.
