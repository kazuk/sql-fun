# tablesample_method_ambiguous

## Code and severity

Code: `tablesample_method_ambiguous`. Severity: Error.

## When it is reported

The sampling-method provider reports more than one eligible handler after
applying visibility rules.

## What analysis knows and does not know

The ambiguity and requested method name are reported. The diagnostic message
does not list or choose a handler automatically.

## How to address it

Qualify the intended method and check the provider catalog for conflicting
eligible definitions.

## Example

Illustrative PostgreSQL analysis: table public.t exists and the provider returns
an ambiguous lookup for sample_method. This metadata state is not provided by
the ordinary CLI context.

```sql
SELECT * FROM public.t TABLESAMPLE sample_method(10);
```

This illustrates an ambiguous provider result, not a promise that two arbitrary
SQL declarations create ambiguity under PostgreSQL visibility rules.

## Related diagnostics

[tablesample_method_not_found](tablesample_method_not_found.md) means no handler
was found. [tablesample_invalid_handler](tablesample_invalid_handler.md)
concerns one unsuitable handler.
