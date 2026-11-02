# Authenticated API through JSON Lines

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

Initialize a selected schema through the CLI using the suite's synthetic catalog,
metadata and original dump hash. The first diagnostic page is retained until this
CLI connection closes; its handle is not reusable in a later CLI invocation.

```scrut
$ set -o pipefail
> jq -c . "$SQL_FUN_API_FIXTURE/initialize.json" | \
>   "$SQL_FUN_BIN" api --user "$USER" --server-command 'exit 99' \
>   2>"$SQL_FUN_API_FIXTURE/initialize.stderr" >"$SQL_FUN_API_FIXTURE/initialize.response"
> jq -c --slurpfile request "$SQL_FUN_API_FIXTURE/initialize.json" '
>   .result as $r | {
>     context_handle:($r.context_handle | test("^context-[0-9]+$")),
>     hash_matches:($r.schema_dump_hash == $request[0].params.expected_hash),
>     total_length:$r.problems.total_length,
>     offset:$r.problems.offset,
>     require_release:$r.problems.require_release,
>     collection_handle:($r.problems.handle | test("^collection-[0-9]+$")),
>     diagnostic:($r.problems.items[0] | {
>       code, statement_span, probrem_span,
>       path_matches:(.file_path == $request[0].params.schema_dump)
>     })
>   }' "$SQL_FUN_API_FIXTURE/initialize.response"
{"context_handle":true,"hash_matches":true,"total_length":2,"offset":0,"require_release":true,"collection_handle":true,"diagnostic":{"code":"table_not_found","statement_span":null,"probrem_span":null,"path_matches":true}}
```

```scrut
$ test ! -s "$SQL_FUN_API_FIXTURE/initialize.stderr"
```

A request with an incorrect expected hash returns an initialization error without
a context handle. JSON-RPC application errors remain JSON Lines responses.

```scrut
$ set -o pipefail
> jq -c '.params.expected_hash = ("0" * 64)' \
>   "$SQL_FUN_API_FIXTURE/initialize.json" | \
>   "$SQL_FUN_BIN" api --user "$USER" --server-command 'exit 99' | \
>   jq -c '{
>     has_result:has("result"),
>     mismatch:(.error.data.SchemaContextInitializeFailed |
>       contains("schema dump changed")),
>     retry_guidance:(.error.message | contains("retry with the current file hash"))
>   }'
{"has_result":false,"mismatch":true,"retry_guidance":true}
```

Request accesses explicitly on the same authenticated connection that owns the
schema context. Default requests remain empty; relation actions and columns
share the four-field collection.

```scrut
$ set -euo pipefail
> coproc ACCESS_API {
>   "$SQL_FUN_BIN" api --user "$USER" --server-command 'exit 99' \
>     2>"$SQL_FUN_API_FIXTURE/access.stderr"
> }
> api_pid=$ACCESS_API_PID
> api_input=${ACCESS_API[1]}
> exec {api_output}<&"${ACCESS_API[0]}"
> sequence=0
> function rpc() {
>   sequence=$((sequence + 1))
>   printf '%s\n' "$1" >&"$api_input"
>   IFS= read -r response <&"$api_output"
>   jq -e --argjson id "$sequence" '
>     .id == $id and has("result") and (has("error") | not)
>   ' <<<"$response" >/dev/null
>   result=$(jq -c '.result' <<<"$response")
> }
> rpc "$(jq -c . "$SQL_FUN_API_FIXTURE/initialize.json")"
> handle=$(jq -r '.context_handle' <<<"$result")
> for enabled in omitted false true; do
>   request=$(jq -cn --arg handle "$handle" --arg enabled "$enabled" '
>     {method:"analyze_query",params:{context_handle:$handle,
>       query:{Text:"SELECT id FROM retained; SELECT 1"}}} |
>     if $enabled == "omitted" then .
>     else .params.analyze_access = ($enabled == "true") end
>   ')
>   rpc "$request"
>   expected=0
>   if [[ "$enabled" == true ]]; then expected=2; fi
>   jq -e --argjson expected "$expected" '
>     .queries.items as $q |
>     ($q[0] | has("accessing_relations") | not) and
>     $q[0].column_access.total_length == $expected and
>     $q[1].column_access.total_length == 0
>   ' <<<"$result" >/dev/null
>   if [[ "$enabled" == true ]]; then
>     jq -e '
>       .queries.items[0].column_access.items as $a |
>       ($a | map(.access_kind) | sort) == ["ReadRows","SelectOutput"] and
>       all($a[]; (keys | sort) ==
>         ["access_kind","column_name","relation","relation_kind"] and
>         .relation_kind == "Table" and
>         .relation == {catalog:null,schema:"public",name:"retained"})
>     ' <<<"$result" >/dev/null
>     echo 'explicit access analysis: relation and column records'
>   fi
>   collection=$(jq -r '.queries.items[0].column_access.handle' <<<"$result")
>   rpc "$(jq -cn --arg handle "$collection" '
>     {method:"release_collection",params:{collection_handle:$handle}}
>   ')"
> done
> exec {api_input}>&-
> cat <&"$api_output" >"$SQL_FUN_API_FIXTURE/access.stdout"
> wait "$api_pid"
> test ! -s "$SQL_FUN_API_FIXTURE/access.stdout"
> test ! -s "$SQL_FUN_API_FIXTURE/access.stderr"
> echo 'default and false: empty; requested zero-access query: empty'
explicit access analysis: relation and column records
default and false: empty; requested zero-access query: empty
```
