CREATE FUNCTION public.parallel_probe() RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION public.parallel_probe(text) RETURNS text LANGUAGE sql AS 'SELECT $1';

ALTER FUNCTION public.parallel_probe() PARALLEL SAFE;
ALTER FUNCTION public.parallel_probe(text) PARALLEL RESTRICTED;

CREATE VIEW public.parallel_result AS SELECT public.parallel_probe() AS value;
