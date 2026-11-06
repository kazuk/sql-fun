# Keep schemas intact: each parameter/result owns a separate $defs namespace.
{methods: [.methods[] | {
  contract: del(.params, .result),
  parameters: [.params[] | {descriptor: del(.schema), schema: .schema}],
  result: (if .result == null then null else
    {descriptor: (.result | del(.schema)), schema: .result.schema} end)
}]}
