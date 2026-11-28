SELECT COALESCE(json_object_agg(
  am.amname, pg_catalog.pg_indexam_has_property(am.oid, 'can_exclude')
  ORDER BY am.amname COLLATE "C"
), '{}'::json)
FROM pg_catalog.pg_am am
WHERE am.amtype = 'i' AND am.oid < 16384
