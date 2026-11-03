# tablesample_invalid_handler

## Code and severity

Code: `tablesample_invalid_handler`. Severity: Error.

## When it is reported

A sampling-method provider identifies a handler that does not satisfy the
required handler signature or return type.

## What analysis knows and does not know

The requested method name and invalid-handler classification are known. The
diagnostic does not establish usable input types or repeatability facts.

## How to address it

Verify the selected declaration is a TABLESAMPLE handler with the PostgreSQL
handler contract, rather than an ordinary same-named function. Correct its
declaration or choose the intended method.

## Example

Illustrative PostgreSQL analysis: table public.t exists and the provider
classifies bad_sample as an invalid handler. Normal CLI analysis lacks these
handler facts.

```sql
SELECT * FROM public.t TABLESAMPLE bad_sample(10);
```

A sampling handler requires an internal argument and tsm_handler result;
ordinary scalar function metadata is insufficient.

## Related diagnostics

[tablesample_metadata_unavailable](tablesample_metadata_unavailable.md) reports
absence of facts, rather than an established invalid handler.
