# Function not found : SF00002

## Situation

 Function or procedure not found.

```SQL
-- calling scalar function
SELECT function_name();

-- calling set of function
SELECT * from function_name()
```

- `function_name` not found in schema sematic analysis context.

## Why

- Type in `function_name`
- PostgreSQL extension not enabled in context.
- When `function_name` defined in schema
  - Schema not specified.
  - Schema not in search path.
