SELECT COALESCE(json_agg(json_build_object(
  'name', am.amname,
  'kind', CASE am.amtype WHEN 't' THEN 'TABLE' WHEN 'i' THEN 'INDEX' END,
  'handler_schema', n.nspname,
  'handler_name', p.proname
) ORDER BY am.amname), '[]'::json)
FROM pg_catalog.pg_am am
JOIN pg_catalog.pg_proc p ON p.oid = am.amhandler
JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
WHERE am.oid < 16384;
