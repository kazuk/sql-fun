CREATE TABLE public.items (id integer PRIMARY KEY, value integer DEFAULT 7);
CREATE VIEW public.item_view AS SELECT id AS key, value, value + 1 AS computed FROM public.items;
CREATE VIEW public.nested (renamed, value) AS SELECT key, value FROM public.item_view;
CREATE TABLE public.diagnostic_items (id integer);
CREATE VIEW public.writable AS SELECT id FROM public.diagnostic_items;
CREATE VIEW public.read_only AS SELECT DISTINCT id FROM public.diagnostic_items;
