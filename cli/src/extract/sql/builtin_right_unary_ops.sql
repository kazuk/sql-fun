with ops as (
  select
    o.oprname,
    r.typname as input_type,
    res.typname as result,
    coalesce(p.proisstrict, false) as strict,
    json_build_object(
      'restriction', restriction.proname,
      'join', join_estimator.proname,
      'join_arity', join_estimator.pronargs,
      'commutator', CASE WHEN commutator.oid IS NOT NULL THEN
        json_build_array(commutator_namespace.nspname, commutator.oprname) END,
      'negator', negator.oprname,
      'hashes', o.oprcanhash,
      'merges', o.oprcanmerge
    ) as settings
  from pg_catalog.pg_operator o
  join pg_catalog.pg_type r on r.oid = o.oprright
  join pg_catalog.pg_type res on res.oid = o.oprresult
  left join pg_catalog.pg_proc p on p.oid = o.oprcode
  left join pg_catalog.pg_proc restriction on restriction.oid = o.oprrest
  left join pg_catalog.pg_proc join_estimator on join_estimator.oid = o.oprjoin
  left join pg_catalog.pg_operator commutator on commutator.oid = o.oprcom
  left join pg_catalog.pg_namespace commutator_namespace
    on commutator_namespace.oid = commutator.oprnamespace
  left join pg_catalog.pg_operator negator on negator.oid = o.oprnegate
  where o.oid < 16384 and o.oprleft = 0 and o.oprright <> 0
),
grouped as (
  select
    oprname,
    json_agg(
      json_build_object(
        'input', input_type,
        'result', result,
        'strict', strict,
        'settings', settings
      )
      order by input_type
    ) as entries
  from ops
  group by oprname
)
select coalesce(json_object_agg(oprname, entries), '{}'::json)
from grouped;
