with base as (
  select
    p.oid,
    p.proname,
    rt.typname as retname,
    p.proisstrict,
    p.prokind,
    p.proretset,
    p.proargtypes,
    p.proargtypes::text as argtypes_text
  from pg_catalog.pg_proc p
  join pg_catalog.pg_namespace n on n.oid = p.pronamespace
  join pg_catalog.pg_type rt on rt.oid = p.prorettype
  where p.oid < 16384 and n.nspname = 'pg_catalog'
    and rt.typname not in ('trigger', 'internal')
    and not exists (
      select 1
      from unnest(p.proargtypes::oid[]) as t(oid)
      join pg_catalog.pg_type at on at.oid = t.oid
      where at.typname = 'internal'
    )
),
args as (
  select
    b.oid,
    array_agg(at.typname order by u.ord) filter (where u.oid is not null) as args
  from base b
  left join lateral unnest(b.proargtypes::oid[]) with ordinality as u(oid, ord)
    on true
  left join pg_catalog.pg_type at on at.oid = u.oid
  group by b.oid
),
funcs as (
  select
    b.proname,
    b.argtypes_text,
    json_build_object(
      'name', b.proname,
      'args', coalesce(a.args, array[]::text[]),
      'ret', b.retname,
      'is_strict', b.proisstrict,
      'kind', b.prokind,
      'returns_set', b.proretset
    ) as obj
  from base b
  left join args a on a.oid = b.oid
),
grouped as (
  select
    proname,
    json_agg(obj order by argtypes_text) as overloads
  from funcs
  group by proname
)
select coalesce(
  json_agg(
    json_build_object('name', proname, 'overloads', overloads)
    order by proname
  ),
  '[]'::json
)
from grouped;
