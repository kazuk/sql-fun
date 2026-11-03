# COMMENT

## Overview

PostgreSQL COMMENT analysis records the supplied object name, comment text and
object kind in the analysis catalog.

```sql
COMMENT ON TABLE public.items IS 'Application items';
```

The operation preserves annotation metadata for consumers; it does not change
query result typing, execute a database command or establish that the target
exists. Routine targets can include argument type spellings as part of their
name.

## Dialect and engine-specific behavior

This is the implemented PostgreSQL annotation path. Recording an object kind or
object name does not constitute full object lookup, signature resolution,
permission validation or complete COMMENT syntax support. SQLite does not
provide this PostgreSQL COMMENT operation.

## Diagnostics

This metadata path does not emit a COMMENT-specific structured diagnostic.
Invalid or unsupported target conversion can fail analysis. Do not infer a
missing-object diagnostic merely because the named object is absent; this path
does not perform that lookup.
