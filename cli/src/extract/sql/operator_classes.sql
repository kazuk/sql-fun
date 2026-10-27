select coalesce(json_agg(json_build_object(
  'schema', n.nspname, 'name', c.opcname, 'access_method', a.amname,
  'family', json_build_object('schema', fn.nspname, 'name', f.opfname, 'access_method', a.amname),
  'input_type', json_build_object('schema', tn.nspname, 'name', t.typname),
  'default', c.opcdefault,
  'key_type', case when c.opckeytype = 0 then null else
    json_build_object('schema', kn.nspname, 'name', k.typname) end
) order by n.nspname::text collate "C", c.opcname::text collate "C", a.amname::text collate "C"), '[]'::json)
from pg_catalog.pg_opclass c
join pg_catalog.pg_namespace n on n.oid = c.opcnamespace
join pg_catalog.pg_am a on a.oid = c.opcmethod
join pg_catalog.pg_opfamily f on f.oid = c.opcfamily and f.opfmethod = c.opcmethod
join pg_catalog.pg_namespace fn on fn.oid = f.opfnamespace
join pg_catalog.pg_type t on t.oid = c.opcintype
join pg_catalog.pg_namespace tn on tn.oid = t.typnamespace
left join pg_catalog.pg_type k on k.oid = c.opckeytype
left join pg_catalog.pg_namespace kn on kn.oid = k.typnamespace
where (c.oid < 16384 and n.nspname = 'pg_catalog')
   or n.nspname::text in (
     select jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb)
   );
