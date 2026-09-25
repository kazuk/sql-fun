with detail as (
  select
    p.oid,
    p.proname,
    p.proallargtypes,
    p.proargmodes,
    p.proargnames
  from pg_catalog.pg_proc p
  join pg_catalog.pg_namespace n on n.oid = p.pronamespace
  where p.oid < 16384 and n.nspname = 'pg_catalog'
    and p.proallargtypes is not null
),
allargs as (
  select
    d.oid,
    array_agg(t.typname order by u.ord)::text as allargtypes
  from detail d
  join lateral unnest(d.proallargtypes) with ordinality as u(oid, ord) on true
  join pg_catalog.pg_type t on t.oid = u.oid
  group by d.oid
)
select coalesce(
  json_agg(
    json_build_object(
      'name', d.proname,
      'allargtypes', a.allargtypes,
      'argmodes', d.proargmodes::text,
      'argnames', d.proargnames::text
    )
    order by d.proname, d.oid
  ),
  '[]'::json
)
from detail d
join allargs a on a.oid = d.oid;
