SELECT COALESCE(json_agg(json_build_object(
  'schema', n.nspname, 'name', t.typname,
  'definition', format_type(t.typbasetype, t.typtypmod)
    || CASE WHEN t.typnotnull THEN ' NOT NULL' ELSE '' END
    || CASE WHEN t.typdefaultbin IS NOT NULL THEN ' DEFAULT ' || pg_get_expr(t.typdefaultbin, 0) ELSE '' END
    || COALESCE((SELECT string_agg(' CONSTRAINT ' || quote_ident(c.conname) || ' ' || pg_get_constraintdef(c.oid), '' ORDER BY c.conname)
                 FROM pg_constraint c WHERE c.contypid = t.oid), '')
) ORDER BY n.nspname, t.typname), '[]'::json)
FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace
WHERE t.typtype = 'd' AND n.nspname IN ('pg_catalog', 'information_schema')
AND t.oid < 16384;
