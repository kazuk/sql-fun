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
    'definition', pg_get_viewdef(vn.oid, true),
    'options', COALESCE((
      SELECT 'WITH (' || string_agg(format('%I = %L', option_name, option_value), ', ' ORDER BY option_name) || ')'
      FROM pg_options_to_table((SELECT reloptions FROM pg_class WHERE oid = vn.oid))
    ), ''),
    'rules', COALESCE((
      SELECT json_agg(json_build_object(
        'prefix', format(E'CREATE RULE %I AS\n    ON %s TO %I', r.rulename,
          CASE r.ev_type WHEN '2' THEN 'UPDATE' WHEN '3' THEN 'INSERT' WHEN '4' THEN 'DELETE' ELSE 'SELECT' END, vn.view),
        'qualified_prefix', format(E'CREATE RULE %I AS\n    ON %s TO %I.%I', r.rulename,
          CASE r.ev_type WHEN '2' THEN 'UPDATE' WHEN '3' THEN 'INSERT' WHEN '4' THEN 'DELETE' ELSE 'SELECT' END, vn.schema, vn.view),
        'definition', pg_get_ruledef(r.oid, true)
      ) ORDER BY r.rulename)
      FROM pg_rewrite r WHERE r.ev_class = vn.oid AND r.rulename <> '_RETURN' AND r.oid < 16384
    ), '[]'::json),
    'write_metadata', COALESCE((
      SELECT string_agg(statement, E'\n' ORDER BY category, name)
      FROM (
        SELECT 1 AS category, a.attname::text AS name,
               format('ALTER VIEW %I.%I ALTER COLUMN %I SET DEFAULT %s;', vn.schema, vn.view, a.attname, pg_get_expr(d.adbin, d.adrelid)) AS statement
        FROM pg_attrdef d JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum
        WHERE d.adrelid = vn.oid AND d.oid < 16384
        UNION ALL
        SELECT 3, t.tgname::text, pg_get_triggerdef(t.oid, true) || ';'
        FROM pg_trigger t WHERE t.tgrelid = vn.oid AND NOT t.tgisinternal AND t.oid < 16384
      ) AS declarations
    ), '')
  )
  ORDER BY md.depth, vn.schema, vn.view
), '[]'::json)
FROM mindepth md
JOIN view_nodes vn ON vn.oid = md.oid;
