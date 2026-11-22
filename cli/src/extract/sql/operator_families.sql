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
    json_build_object('schema', n.nspname, 'name', c.opcname, 'access_method', a.amname) AS identity,
    (c.oid < 16384 AND n.nspname = 'pg_catalog')
      OR n.nspname::text IN (SELECT name FROM selected_schemas) AS selected
  FROM pg_catalog.pg_opclass c
  JOIN pg_catalog.pg_namespace n ON n.oid = c.opcnamespace
  JOIN pg_catalog.pg_am a ON a.oid = c.opcmethod
), type_keys AS (
  SELECT t.oid, json_build_object(
    'schema', n.nspname, 'name', t.typname,
    'array_element', CASE WHEN el.oid IS NULL THEN NULL ELSE
      json_build_object('schema', en.nspname, 'name', el.typname) END
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
  SELECT m.amopfamily AS family, json_agg(json_build_object(
    'number', m.amopstrategy,
    'left_type', lt.identity, 'right_type', rt.identity,
    'operator', json_build_object('schema', n.nspname, 'name', o.oprname),
    'order_family', CASE WHEN sort.oid IS NULL THEN NULL ELSE
      json_build_object('schema', sort.schema, 'name', sort.name, 'access_method', sort.access_method) END,
    'owner', own.identity, 'builtin', m.oid < 16384
  )::jsonb || CASE WHEN hard.objid IS NULL THEN '{}'::jsonb
    ELSE '{"family_owned":true}'::jsonb END
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
  SELECT m.amprocfamily AS family, json_agg(json_build_object(
    'number', m.amprocnum,
    'left_type', lt.identity, 'right_type', rt.identity,
    'function', json_build_object('schema', n.nspname, 'name', p.proname),
    'arguments', COALESCE((
      SELECT json_agg(arg_type.identity ORDER BY arg.position)
      FROM unnest(p.proargtypes) WITH ORDINALITY arg(type_oid, position)
      JOIN type_keys arg_type ON arg_type.oid = arg.type_oid
    ), '[]'::json),
    'owner', own.identity, 'builtin', m.oid < 16384
  )::jsonb || CASE WHEN hard.objid IS NULL THEN '{}'::jsonb
    ELSE '{"family_owned":true}'::jsonb END
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
