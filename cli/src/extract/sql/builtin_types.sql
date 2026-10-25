with identities as (
  select t.oid,
    json_build_object('Concrete', json_build_object(
      'base', json_build_object('catalog', null, 'schema', coalesce(en.nspname, n.nspname),
        'name', coalesce(e.typname, t.typname)),
      'is_array', e.oid is not null)) as reference
  from pg_catalog.pg_type t
  join pg_catalog.pg_namespace n on n.oid = t.typnamespace
  left join pg_catalog.pg_type e on e.oid = t.typelem and e.typarray = t.oid
  left join pg_catalog.pg_namespace en on en.oid = e.typnamespace
), families as (
  select r.rngtypid, r.rngmultitypid,
    json_build_object('range', rt.reference, 'subtype', st.reference,
      'multirange', mt.reference) as relationship
  from pg_catalog.pg_range r
  join identities rt on rt.oid = r.rngtypid
  join identities st on st.oid = r.rngsubtype
  join identities mt on mt.oid = r.rngmultitypid
)
select coalesce(json_agg(
  (jsonb_build_object('name', t.typname, 'schema', n.nspname,
    'category', case when t.typisdefined then t.typcategory::text else null end)
    || case when f.relationship is not null
      then jsonb_build_object('range_relationship', f.relationship) else '{}'::jsonb end)
    order by t.oid), '[]'::json)
from pg_catalog.pg_type t
join pg_catalog.pg_namespace n on n.oid = t.typnamespace
left join families f on f.rngtypid = t.oid
where (t.oid < 16384 and n.nspname = 'pg_catalog'
    and t.typtype <> 'c'
    and t.typname !~ '^__'
    and t.typname !~ '^pg_toast_'
    and t.typname !~ '^pg_temp_'
    and t.typname !~ '^pg_toast_temp_'
    and (t.typname !~ '^_' or t.typname = '_record'))
  or (t.typtype in ('r', 'm') and n.nspname::text in (
    select jsonb_array_elements_text(
      coalesce(current_setting('sql_fun.definition_schemas', true), '[]')::jsonb)))
