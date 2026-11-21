CREATE VIEW public.drop_view_base AS SELECT 1 AS value;
CREATE VIEW public.drop_view_child AS SELECT value FROM public.drop_view_base;
CREATE MATERIALIZED VIEW public.drop_view_leaf AS SELECT value FROM public.drop_view_child;
DROP VIEW public.drop_view_base CASCADE;
CREATE VIEW public.drop_view_base AS SELECT 'recreated'::text AS value;

CREATE VIEW public.drop_view_group AS SELECT 1 AS value;
CREATE VIEW public.drop_view_group_child AS SELECT value FROM public.drop_view_group;
DROP VIEW IF EXISTS public.drop_view_group, public.drop_view_group_child, public.drop_view_group, public.drop_view_missing;

CREATE VIEW public.drop_view_retained AS SELECT 1 AS value;
CREATE VIEW public.drop_view_retained_child AS SELECT value FROM public.drop_view_retained;
CREATE TABLE public.drop_view_wrong_kind (value integer);
