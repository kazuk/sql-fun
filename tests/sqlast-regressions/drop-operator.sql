CREATE FUNCTION public.drop_operator_binary(integer, integer)
RETURNS integer LANGUAGE sql AS 'SELECT $1';
CREATE FUNCTION public.drop_operator_prefix(integer)
RETURNS integer LANGUAGE sql AS 'SELECT $1';
CREATE OPERATOR public.%% (
    LEFTARG = integer, RIGHTARG = integer, FUNCTION = public.drop_operator_binary
);
CREATE OPERATOR public.%% (
    RIGHTARG = integer, FUNCTION = public.drop_operator_prefix
);
CREATE VIEW public.drop_operator_view AS SELECT 1 OPERATOR(public.%%) 2 AS n;
DROP OPERATOR IF EXISTS public.%% (integer, integer), public.%% (text, text) CASCADE;
