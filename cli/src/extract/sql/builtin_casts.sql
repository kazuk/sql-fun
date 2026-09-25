with casts as (
  select
    src.typname as source,
    tgt.typname as target,
    case
      when c.castfunc = 0 then '0'
      else (
        select p.proname || '(' || coalesce(array_to_string(a.args, ','), '') || ')'
        from pg_catalog.pg_proc p
        left join lateral (
          select array_agg(t.typname order by u.ord) as args
          from unnest(p.proargtypes::oid[]) with ordinality as u(oid, ord)
          join pg_catalog.pg_type t on t.oid = u.oid
        ) a on true
        where p.oid = c.castfunc
      )
    end as func,
    c.castmethod::text as method,
    c.castcontext::text as context,
    tgt.typispreferred as is_prefferd
  from pg_catalog.pg_cast c
  join pg_catalog.pg_type src on src.oid = c.castsource
  join pg_catalog.pg_type tgt on tgt.oid = c.casttarget
  where c.oid < 16384
),
grouped as (
  select
    source,
    json_object_agg(
      target,
      json_build_object(
        'func', func,
        'method', method,
        'context', context,
        'is_prefferd', is_prefferd
      )
    ) as targets
  from casts
  group by source
)
select coalesce(json_object_agg(source, targets), '{}'::json)
from grouped;
