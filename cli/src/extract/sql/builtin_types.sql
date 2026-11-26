WITH bootstrap AS (
  SELECT t.oid
  FROM pg_catalog.pg_type t
  WHERE t.oid < 16384
    AND t.typnamespace = 'pg_catalog'::regnamespace
    AND t.typtype <> 'c'
    AND t.typname !~ '^__'
    AND t.typname !~ '^pg_toast_'
    AND t.typname !~ '^pg_temp_'
    AND t.typname !~ '^pg_toast_temp_'
    AND (t.typname !~ '^_' OR t.typname = '_record')
), selected AS (
  SELECT t.*, n.nspname, b.oid IS NOT NULL AS is_bootstrap
  FROM pg_catalog.pg_type t
  JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
  LEFT JOIN bootstrap b ON b.oid = t.oid
  WHERE b.oid IS NOT NULL
     OR (
       t.typtype = 'b'
       AND n.nspname <> 'pg_toast'
       AND n.nspname !~ '^pg_(toast_)?temp_[0-9]+$'
       AND NOT EXISTS (
         SELECT 1 FROM pg_catalog.pg_type element
         WHERE element.typarray = t.oid
       )
     )
)
SELECT coalesce(json_agg(json_build_object(
  'schema', selected.nspname,
  'name', selected.typname,
  'category', CASE WHEN selected.typisdefined THEN selected.typcategory::text ELSE NULL END,
  'length', CASE WHEN selected.typisdefined THEN selected.typlen ELSE NULL END,
  'alterable_base', selected.typisdefined AND selected.typtype = 'b'
    AND NOT (selected.typelem <> 0 AND selected.typsubscript =
      'pg_catalog.array_subscript_handler(internal)'::regprocedure),
  'storage', CASE WHEN selected.typisdefined THEN selected.typstorage::text ELSE NULL END,
  'alignment', CASE WHEN selected.typisdefined THEN selected.typalign::text ELSE NULL END,
  'by_value', CASE WHEN selected.typisdefined THEN selected.typbyval ELSE NULL END,
  'array_name', CASE WHEN a.oid IS NOT NULL THEN json_build_object(
    'schema', an.nspname, 'name', a.typname
  ) END,
  'collatable', CASE WHEN selected.typisdefined THEN selected.typcollation <> 0 ELSE NULL END
) ORDER BY
  CASE WHEN is_bootstrap THEN 0 ELSE 1 END,
  CASE WHEN is_bootstrap THEN selected.oid ELSE NULL END,
  selected.nspname::text COLLATE "C",
  selected.typname::text COLLATE "C"
), '[]'::json)
FROM selected
LEFT JOIN pg_catalog.pg_type a ON a.oid = selected.typarray
LEFT JOIN pg_catalog.pg_namespace an ON an.oid = a.typnamespace;
