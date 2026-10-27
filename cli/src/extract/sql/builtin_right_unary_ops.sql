with ops as (
  select
    o.oprname,
    r.typname as input_type,
    res.typname as result,
    coalesce(p.proisstrict, false) as strict
  from pg_catalog.pg_operator o
  join pg_catalog.pg_type r on r.oid = o.oprright
  join pg_catalog.pg_type res on res.oid = o.oprresult
  left join pg_catalog.pg_proc p on p.oid = o.oprcode
  where o.oid < 16384 and o.oprleft = 0 and o.oprright <> 0
),
grouped as (
  select
    oprname,
    json_agg(
      json_build_object(
        'input', input_type,
        'result', result,
        'strict', strict
      )
      order by input_type
    ) as entries
  from ops
  group by oprname
)
select coalesce(json_object_agg(oprname, entries), '{}'::json)
from grouped;
