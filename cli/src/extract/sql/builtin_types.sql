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
  'schema', nspname,
  'name', typname,
  'category', CASE WHEN typisdefined THEN typcategory::text ELSE NULL END,
  'collatable', CASE WHEN typisdefined THEN typcollation <> 0 ELSE NULL END
) ORDER BY
  CASE WHEN is_bootstrap THEN 0 ELSE 1 END,
  CASE WHEN is_bootstrap THEN oid ELSE NULL END,
  nspname::text COLLATE "C",
  typname::text COLLATE "C"
), '[]'::json)
FROM selected;
