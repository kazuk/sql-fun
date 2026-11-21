CREATE FUNCTION public.drop_probe(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1';
CREATE FUNCTION public.drop_probe(text) RETURNS text LANGUAGE sql AS 'SELECT $1';
CREATE VIEW public.drop_probe_view AS SELECT public.drop_probe(1) AS value;
CREATE MATERIALIZED VIEW public.drop_probe_indirect AS SELECT value FROM public.drop_probe_view;

DROP FUNCTION public.drop_probe(IN integer) CASCADE;

CREATE FUNCTION public.drop_recreate() RETURNS integer LANGUAGE sql AS 'SELECT 1';
DROP FUNCTION public.drop_recreate();
CREATE FUNCTION public.drop_recreate() RETURNS text LANGUAGE sql AS 'SELECT ''recreated''';
CREATE VIEW public.drop_recreate_view AS SELECT public.drop_recreate() AS value;

CREATE FUNCTION public.drop_poly(anyelement) RETURNS anyelement LANGUAGE sql AS 'SELECT $1';
CREATE FUNCTION public.drop_poly(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1';
CREATE VIEW public.drop_poly_view AS SELECT public.drop_poly(1) AS value;

CREATE FUNCTION public.drop_bound(anyelement) RETURNS anyelement LANGUAGE sql AS 'SELECT $1';
CREATE VIEW public.drop_bound_view AS SELECT public.drop_bound(1) AS value;
CREATE FUNCTION public.drop_bound(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1';
DROP FUNCTION public.drop_bound(anyelement) CASCADE;
