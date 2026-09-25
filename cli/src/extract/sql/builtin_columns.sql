  select coalesce(json_agg(t),'[]'::json)
  from (
    select
      n.nspname  as schema,
      c.relname  as table,
      a.attnum   as ordinal,
      a.attname  as column,
      case when ty.typtype='p' then ty.typname
           else format_type(a.atttypid,a.atttypmod)
      end        as data_type,
      a.attnotnull as not_null,
      pg_get_expr(d.adbin,d.adrelid) as default_expr
    from pg_catalog.pg_class c
    join pg_catalog.pg_namespace n on n.oid=c.relnamespace
    join pg_catalog.pg_attribute a on a.attrelid=c.oid
    join pg_catalog.pg_type ty on ty.oid=a.atttypid
    left join pg_catalog.pg_attrdef d on d.adrelid=a.attrelid and d.adnum=a.attnum
    where c.oid < 16384 and c.relkind='r' and a.attnum>0 and not a.attisdropped
      and n.nspname in ('pg_catalog','information_schema')
    order by 1,2,3
  ) t;
