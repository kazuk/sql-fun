select json_build_object(
  'database', (
    select json_build_object(
      'encoding', d.encoding, 'encoding_name', pg_catalog.pg_encoding_to_char(d.encoding),
      'provider', d.datlocprovider, 'collate', d.datcollate, 'ctype', d.datctype,
      'locale', d.datlocale, 'icu_rules', d.daticurules, 'version', d.datcollversion,
      'default_collation', json_build_object('schema', n.nspname, 'name', c.collname, 'encoding', c.collencoding)
    )
    from pg_catalog.pg_database d
    join pg_catalog.pg_collation c on c.collprovider = 'd'
    join pg_catalog.pg_namespace n on n.oid = c.collnamespace
    where d.datname = pg_catalog.current_database()
  ),
  'collations', coalesce((
    select json_agg(json_build_object(
      'schema', n.nspname, 'name', c.collname, 'encoding', c.collencoding,
      'provider', c.collprovider, 'deterministic', c.collisdeterministic,
      'collate', c.collcollate, 'ctype', c.collctype, 'locale', c.colllocale,
      'icu_rules', c.collicurules, 'version', c.collversion
    ) order by n.nspname::text collate "C", c.collname::text collate "C", c.collencoding)
    from pg_catalog.pg_collation c
    join pg_catalog.pg_namespace n on n.oid = c.collnamespace
    where (c.oid < 16384 and n.nspname = 'pg_catalog')
       or n.nspname::text in (
         select jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb)
       )
  ), '[]'::json)
);
