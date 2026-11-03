# index_invalid_alteration

## Code and severity

Code: `index_invalid_alteration`. Severity: Error.

## When it is reported

ALTER INDEX fails target or operation validation: a missing or wrong-kind
relation, invalid option namespace/name/value, duplicate SET option, RESET
value, invalid column ordinal, statistics on a non-expression or included
column, or statistics below -1.

## What analysis knows and does not know

The index identity, rejection kind and relevant option/value or column ordinal
are retained. A rejected statement does not publish a replacement index
definition.

## How to address it

Use the reason in the message to correct the target, B-tree storage option or
value. Statistics require a one-based expression-key column ordinal; use -1 for
the default. Do not treat an unsupported access-method implementation as this
semantic diagnostic.

## Example

PostgreSQL; the schema contains B-tree index `public.t_expr_idx` on `(a+1)`.

```sql
ALTER INDEX public.t_expr_idx SET (fillfactor = 1);
```

The B-tree fillfactor is outside its accepted range. Other rejection reasons
share this code.

## Related diagnostics

[index_statistics_clamped](index_statistics_clamped.md) permits a statistics
request after reducing it to 10000. [index_not_found](index_not_found.md) is a
separate name-lookup diagnostic.
