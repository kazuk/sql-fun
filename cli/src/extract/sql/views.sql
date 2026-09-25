WITH RECURSIVE view_nodes AS (
  SELECT c.oid, n.nspname AS schema, c.relname AS view,
         CASE c.relkind WHEN 'm' THEN 'materialized' ELSE 'view' END AS kind
  FROM pg_class c
  JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.oid < 16384 AND c.relkind IN ('v','m')
    AND n.nspname IN ('pg_catalog','information_schema')
),
edges AS (
  -- view -> referenced view
  SELECT DISTINCT v.oid AS view_oid, r.oid AS ref_oid
  FROM pg_rewrite rw
  JOIN pg_class v  ON v.oid = rw.ev_class AND v.relkind IN ('v','m')
  JOIN pg_namespace nv ON nv.oid = v.relnamespace
  JOIN pg_depend d ON d.classid = 'pg_rewrite'::regclass
                   AND d.objid   = rw.oid
                   AND d.refclassid = 'pg_class'::regclass
                   AND d.deptype = 'n'
  JOIN pg_class r  ON r.oid = d.refobjid AND r.relkind IN ('v','m')
  JOIN pg_namespace nr ON nr.oid = r.relnamespace
  WHERE v.oid <> r.oid AND v.oid < 16384 AND r.oid < 16384 AND nv.nspname IN ('pg_catalog','information_schema')
    AND nr.nspname IN ('pg_catalog','information_schema')
),
roots AS (
  SELECT vn.oid
  FROM view_nodes vn
  WHERE NOT EXISTS (SELECT 1 FROM edges e WHERE e.view_oid = vn.oid)
),
walk AS (
  SELECT r.oid, 0 AS depth FROM roots r
  UNION ALL
  SELECT e.view_oid, w.depth + 1
  FROM edges e
  JOIN walk  w ON e.ref_oid = w.oid
),
mindepth AS (
  SELECT oid, MAX(depth) AS depth
  FROM walk GROUP BY oid
)
SELECT COALESCE(json_agg(
  json_build_object(
    'schema', vn.schema,
    'view',   vn.view,
    'kind',   vn.kind,
    'definition', pg_get_viewdef(vn.oid, true)
  )
  ORDER BY md.depth, vn.schema, vn.view
), '[]'::json)
FROM mindepth md
JOIN view_nodes vn ON vn.oid = md.oid;
