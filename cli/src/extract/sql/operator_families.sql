WITH selected_schemas AS (
  SELECT jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb) AS name
), family_keys AS (
  SELECT f.oid, n.nspname AS schema, f.opfname AS name, a.amname AS access_method,
    (f.oid < 16384 AND n.nspname = 'pg_catalog')
      OR n.nspname::text IN (SELECT name FROM selected_schemas) AS selected
  FROM pg_catalog.pg_opfamily f
  JOIN pg_catalog.pg_namespace n ON n.oid = f.opfnamespace
  JOIN pg_catalog.pg_am a ON a.oid = f.opfmethod
), class_keys AS (
  SELECT c.oid, c.opcfamily,
    json_build_array(n.nspname, c.opcname) AS identity,
    (c.oid < 16384 AND n.nspname = 'pg_catalog')
      OR n.nspname::text IN (SELECT name FROM selected_schemas) AS selected
  FROM pg_catalog.pg_opclass c
  JOIN pg_catalog.pg_namespace n ON n.oid = c.opcnamespace
  JOIN pg_catalog.pg_am a ON a.oid = c.opcmethod
), type_keys AS (
  SELECT t.oid, json_build_array(
    COALESCE(en.nspname, n.nspname), COALESCE(el.typname, t.typname),
    el.oid IS NOT NULL
  ) AS identity
  FROM pg_catalog.pg_type t
  JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
  LEFT JOIN pg_catalog.pg_type el ON el.typarray = t.oid
  LEFT JOIN pg_catalog.pg_namespace en ON en.oid = el.typnamespace
), owner_dependencies AS (
  SELECT d.classid, d.objid, c.identity, c.selected
  FROM pg_catalog.pg_depend d
  JOIN class_keys c ON c.oid = d.refobjid
  WHERE d.refclassid = 'pg_catalog.pg_opclass'::regclass AND d.deptype = 'i'
), family_dependencies AS (
  SELECT d.classid, d.objid, d.refobjid
  FROM pg_catalog.pg_depend d
  WHERE d.refclassid = 'pg_catalog.pg_opfamily'::regclass AND d.deptype = 'i'
), operator_members AS (
  SELECT m.amopfamily AS family, json_agg(json_build_array(
    m.amopstrategy, lt.identity, rt.identity, json_build_array(n.nspname, o.oprname),
    CASE WHEN sort.oid IS NOT NULL THEN json_build_array(sort.schema, sort.name) END,
    CASE WHEN m.oid < 16384 THEN NULL
      WHEN own.identity IS NOT NULL THEN json_build_object('class', own.identity)
      WHEN hard.objid IS NOT NULL THEN '"family"'::json ELSE '"loose"'::json END
  )
  ORDER BY m.amopstrategy, lt.identity::text COLLATE "C", rt.identity::text COLLATE "C") AS members
  FROM pg_catalog.pg_amop m
  JOIN family_keys f ON f.oid = m.amopfamily
  JOIN type_keys lt ON lt.oid = m.amoplefttype
  JOIN type_keys rt ON rt.oid = m.amoprighttype
  JOIN pg_catalog.pg_operator o ON o.oid = m.amopopr
  JOIN pg_catalog.pg_namespace n ON n.oid = o.oprnamespace
  LEFT JOIN family_keys sort ON sort.oid = m.amopsortfamily
  LEFT JOIN owner_dependencies own ON own.classid = 'pg_catalog.pg_amop'::regclass AND own.objid = m.oid
  LEFT JOIN family_dependencies hard
    ON hard.classid = 'pg_catalog.pg_amop'::regclass AND hard.objid = m.oid AND hard.refobjid = f.oid
  WHERE m.oid < 16384 OR own.selected OR
    (own.identity IS NULL AND f.schema::text IN (SELECT name FROM selected_schemas))
  GROUP BY m.amopfamily
), function_members AS (
  SELECT m.amprocfamily AS family, json_agg(json_build_array(
    m.amprocnum, lt.identity, rt.identity, json_build_array(n.nspname, p.proname),
    COALESCE((
      SELECT json_agg(arg_type.identity ORDER BY arg.position)
      FROM unnest(p.proargtypes) WITH ORDINALITY arg(type_oid, position)
      JOIN type_keys arg_type ON arg_type.oid = arg.type_oid
    ), '[]'::json),
    CASE WHEN m.oid < 16384 THEN NULL
      WHEN own.identity IS NOT NULL THEN json_build_object('class', own.identity)
      WHEN hard.objid IS NOT NULL THEN '"family"'::json ELSE '"loose"'::json END
  )
  ORDER BY m.amprocnum, lt.identity::text COLLATE "C", rt.identity::text COLLATE "C") AS members
  FROM pg_catalog.pg_amproc m
  JOIN family_keys f ON f.oid = m.amprocfamily
  JOIN type_keys lt ON lt.oid = m.amproclefttype
  JOIN type_keys rt ON rt.oid = m.amprocrighttype
  JOIN pg_catalog.pg_proc p ON p.oid = m.amproc
  JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
  LEFT JOIN owner_dependencies own ON own.classid = 'pg_catalog.pg_amproc'::regclass AND own.objid = m.oid
  LEFT JOIN family_dependencies hard
    ON hard.classid = 'pg_catalog.pg_amproc'::regclass AND hard.objid = m.oid AND hard.refobjid = f.oid
  WHERE m.oid < 16384 OR own.selected OR
    (own.identity IS NULL AND f.schema::text IN (SELECT name FROM selected_schemas))
  GROUP BY m.amprocfamily
)
SELECT COALESCE(json_agg(json_build_object(
  'schema', f.schema, 'name', f.name, 'access_method', f.access_method,
  'operators', COALESCE(op.members, '[]'::json),
  'functions', COALESCE(fn.members, '[]'::json)
) ORDER BY f.schema::text COLLATE "C", f.name::text COLLATE "C", f.access_method::text COLLATE "C"), '[]'::json)
FROM family_keys f
LEFT JOIN operator_members op ON op.family = f.oid
LEFT JOIN function_members fn ON fn.family = f.oid
WHERE f.selected OR EXISTS (SELECT 1 FROM class_keys c WHERE c.selected AND c.opcfamily = f.oid);
