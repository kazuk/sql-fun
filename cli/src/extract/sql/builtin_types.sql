select coalesce(
  json_agg(
    json_build_object('name', t.typname)
    order by t.oid
  ),
  '[]'::json
)
from pg_catalog.pg_type t
where t.oid < 16384 and t.typnamespace = 'pg_catalog'::regnamespace
  and t.typtype <> 'c'
  and t.typname !~ '^__'
  and t.typname !~ '^pg_toast_'
  and t.typname !~ '^pg_temp_'
  and t.typname !~ '^pg_toast_temp_'
  and (t.typname !~ '^_' or t.typname = '_record');
