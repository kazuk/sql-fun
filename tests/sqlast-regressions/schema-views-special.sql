CREATE TABLE public.items (id integer PRIMARY KEY);
CREATE MATERIALIZED VIEW public.mv AS SELECT id FROM public.items;
CREATE VIEW public.alias_view AS SELECT x FROM public.items AS i(x);
CREATE VIEW public.alias_nested AS SELECT y FROM public.alias_view AS v(y);
CREATE VIEW public.alias_cast AS SELECT x::integer AS x FROM public.items AS i(x);
