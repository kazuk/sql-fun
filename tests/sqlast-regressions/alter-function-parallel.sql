CREATE FUNCTION public.parallel_probe() RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION public.parallel_probe(text) RETURNS text LANGUAGE sql AS 'SELECT $1';

ALTER FUNCTION public.parallel_probe() PARALLEL SAFE;
ALTER FUNCTION public.parallel_probe(text) PARALLEL RESTRICTED;

CREATE VIEW public.parallel_result AS SELECT public.parallel_probe() AS value;

CREATE FUNCTION public.array_by_catalog_name(_text)
RETURNS pg_catalog._text LANGUAGE sql AS 'SELECT $1';
ALTER FUNCTION public.array_by_catalog_name(text[]) PARALLEL SAFE;

CREATE TYPE public._text AS ENUM ('scalar');
CREATE FUNCTION public.array_shadow_probe(public._text)
RETURNS integer LANGUAGE sql AS 'SELECT 1';
ALTER FUNCTION public.array_shadow_probe(public._text) PARALLEL SAFE;
ALTER FUNCTION public.array_by_catalog_name(_text) PARALLEL SAFE;
