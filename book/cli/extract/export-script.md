# `sql-fun extract export-script`

Print the administrator's extension-file export script to standard output:

```text
sql-fun extract export-script
```

The command has no arguments or options other than `-h` / `--help`. Save its
output before transferring it to the PostgreSQL administrator:

```sh
sql-fun extract export-script > export-extensions.sh
```

The command exits with status 0 on success, nonzero on output failure, and 2
for invalid CLI usage. Running the resulting script has separate prerequisites;
see [the extraction guide](../extract.md#obtain-extension-files-from-an-administrator).
[Common options](../index.md#common-options) are specified before `extract`.
