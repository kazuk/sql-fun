SELECT COALESCE(json_agg(json_build_object(
  'schema', n.nspname, 'name', p.proname,
  'arguments', pg_get_function_arguments(p.oid),
  'result', pg_get_function_result(p.oid)
) ORDER BY n.nspname, p.proname, p.oid::regprocedure::text), '[]'::json)
FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'information_schema' AND p.prokind = 'f' AND p.oid < 16384;
