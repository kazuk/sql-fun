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

CREATE VIEW public.drop_view_row_type AS SELECT 1 AS n;
CREATE VIEW public.drop_view_default_owner AS SELECT true AS b;
ALTER VIEW public.drop_view_default_owner ALTER COLUMN b SET DEFAULT
    ((NULL::public.drop_view_row_type) IS NULL);
CREATE FUNCTION public.drop_view_sql_body() RETURNS integer LANGUAGE sql RETURN 1;
CREATE VIEW public.drop_view_unrelated AS SELECT 1 AS n;
DROP VIEW public.drop_view_unrelated;

CREATE VIEW public.drop_view_replaced_source AS SELECT 1 AS n;
CREATE VIEW public.drop_view_replaced AS SELECT n FROM public.drop_view_replaced_source;
CREATE OR REPLACE VIEW public.drop_view_replaced AS SELECT 2 AS n;
DROP VIEW public.drop_view_replaced_source;
