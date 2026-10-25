select json_build_object('format_version', 1, 'memberships', coalesce(json_agg(json_build_object(
  'negated_by', (select coalesce(json_agg(json_build_object('schema', alias_ns.nspname, 'name', alias_op.oprname)
    order by alias_ns.nspname::text collate "C", alias_op.oprname::text collate "C"), '[]'::json)
    from pg_catalog.pg_operator alias_op join pg_catalog.pg_namespace alias_ns on alias_ns.oid = alias_op.oprnamespace
    where alias_op.oprnegate = o.oid and alias_op.oprleft = o.oprleft and alias_op.oprright = o.oprright
      and ((alias_op.oid < 16384 and alias_ns.nspname = 'pg_catalog')
        or alias_ns.nspname::text in (select jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb)))),
  'negator', case when neg.oid is null then null else json_build_object('schema', nn.nspname, 'name', neg.oprname) end,
  'operator', json_build_object('schema', onsp.nspname, 'name', o.oprname),
  'left', json_build_object('name', json_build_object('schema', oln.nspname, 'name', ol.typname), 'array', ola.oid is not null),
  'right', json_build_object('name', json_build_object('schema', orn.nspname, 'name', ort.typname), 'array', ora.oid is not null),
  'family', json_build_object('schema', fn.nspname, 'name', f.opfname, 'access_method', a.amname),
  'family_left', json_build_object('schema', ln.nspname, 'name', lt.typname),
  'family_right', json_build_object('schema', rn.nspname, 'name', rt.typname),
  'strategy', m.amopstrategy, 'purpose', m.amoppurpose,
  'sort_family', case when sf.oid is null then null else json_build_object(
    'schema', sn.nspname, 'name', sf.opfname, 'access_method', sa.amname) end
) order by onsp.nspname::text collate "C", o.oprname::text collate "C",
  o.oprleft::regtype::text collate "C", o.oprright::regtype::text collate "C",
  fn.nspname::text collate "C", f.opfname::text collate "C", m.amopstrategy, m.amoppurpose), '[]'::json))
from pg_catalog.pg_amop m
join pg_catalog.pg_operator o on o.oid = m.amopopr
left join pg_catalog.pg_operator neg on neg.oid = o.oprnegate
left join pg_catalog.pg_namespace nn on nn.oid = neg.oprnamespace
join pg_catalog.pg_type left_declared_type on left_declared_type.oid = o.oprleft
left join pg_catalog.pg_type ola on ola.typarray = left_declared_type.oid and ola.oid = left_declared_type.typelem
join pg_catalog.pg_type ol on ol.oid = coalesce(ola.oid, left_declared_type.oid)
join pg_catalog.pg_namespace oln on oln.oid = ol.typnamespace
join pg_catalog.pg_type ortt on ortt.oid = o.oprright
left join pg_catalog.pg_type ora on ora.typarray = ortt.oid and ora.oid = ortt.typelem
join pg_catalog.pg_type ort on ort.oid = coalesce(ora.oid, ortt.oid)
join pg_catalog.pg_namespace orn on orn.oid = ort.typnamespace
join pg_catalog.pg_namespace onsp on onsp.oid = o.oprnamespace
join pg_catalog.pg_opfamily f on f.oid = m.amopfamily
join pg_catalog.pg_namespace fn on fn.oid = f.opfnamespace
join pg_catalog.pg_am a on a.oid = m.amopmethod and a.oid = f.opfmethod
join pg_catalog.pg_type lt on lt.oid = m.amoplefttype
join pg_catalog.pg_namespace ln on ln.oid = lt.typnamespace
join pg_catalog.pg_type rt on rt.oid = m.amoprighttype
join pg_catalog.pg_namespace rn on rn.oid = rt.typnamespace
left join pg_catalog.pg_opfamily sf on sf.oid = m.amopsortfamily
left join pg_catalog.pg_namespace sn on sn.oid = sf.opfnamespace
left join pg_catalog.pg_am sa on sa.oid = sf.opfmethod
where a.amname = 'btree' and (
  (m.oid < 16384 and o.oid < 16384 and f.oid < 16384 and onsp.nspname = 'pg_catalog' and fn.nspname = 'pg_catalog')
  or exists (select 1 from pg_catalog.pg_opclass selected_class join pg_catalog.pg_namespace selected_ns on selected_ns.oid = selected_class.opcnamespace where selected_class.opcfamily = f.oid and selected_ns.nspname::text in (select jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb)))
  or fn.nspname::text in (select jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb))
  or onsp.nspname::text in (select jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb))
);
