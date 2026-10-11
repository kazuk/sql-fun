# Authenticated API echo through JSON Lines

The suite setup starts a real server in a fresh test-owned user namespace and
cleans up its own child and registration directory when the suite runner exits.
The failing launch override below ensures the CLI reuses that server.

Submit object and array parameters with nested JSON values. Compare complete
response envelopes by ID, independently of callback output order, and check that
successful EOF/drain leaves diagnostics off stdout and stderr.

```scrut
$ set -o pipefail
> printf '%s\n' \
>   '{"method":"echo","params":{"nested":[null,true,{"n":42}]}}' \
>   '{"jsonrpc":"2.0","method":"echo","params":[1,{"x":[false,null]}]}' | \
>   "$SQL_FUN_BIN" api --user "$USER" --server-command 'exit 99' \
>   2>"$SQL_FUN_API_FIXTURE/cli.stderr" | jq -scS 'sort_by(.id)'
[{"id":1,"jsonrpc":"2.0","result":{"nested":[null,true,{"n":42}]}},{"id":2,"jsonrpc":"2.0","result":[1,{"x":[false,null]}]}]
```

```scrut
$ test ! -s "$SQL_FUN_API_FIXTURE/cli.stderr"
```
