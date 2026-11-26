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

-- Use minimal operand types and implementations for the bundled upgrade signatures.
CREATE DOMAIN public.cube AS integer;
CREATE FUNCTION public.drop_operator_cube(public.cube, public.cube)
RETURNS boolean LANGUAGE sql AS 'SELECT true';
CREATE OPERATOR public.@ (LEFTARG = public.cube, RIGHTARG = public.cube, FUNCTION = public.drop_operator_cube);
DROP OPERATOR @ (cube, cube);
CREATE OPERATOR public.~ (LEFTARG = public.cube, RIGHTARG = public.cube, FUNCTION = public.drop_operator_cube);
DROP OPERATOR ~ (cube, cube);
CREATE DOMAIN public.hstore AS integer;
CREATE FUNCTION public.drop_operator_hstore(public.hstore, public.hstore)
RETURNS boolean LANGUAGE sql AS 'SELECT true';
CREATE OPERATOR public.@ (LEFTARG = public.hstore, RIGHTARG = public.hstore, FUNCTION = public.drop_operator_hstore);
DROP OPERATOR @ (hstore, hstore);
CREATE OPERATOR public.~ (LEFTARG = public.hstore, RIGHTARG = public.hstore, FUNCTION = public.drop_operator_hstore);
DROP OPERATOR ~ (hstore, hstore);
CREATE DOMAIN public.seg AS integer;
CREATE FUNCTION public.drop_operator_seg(public.seg, public.seg)
RETURNS boolean LANGUAGE sql AS 'SELECT true';
CREATE OPERATOR public.@ (LEFTARG = public.seg, RIGHTARG = public.seg, FUNCTION = public.drop_operator_seg);
DROP OPERATOR @ (seg, seg);
CREATE OPERATOR public.~ (LEFTARG = public.seg, RIGHTARG = public.seg, FUNCTION = public.drop_operator_seg);
DROP OPERATOR ~ (seg, seg);
CREATE FUNCTION public.drop_operator__int4(integer[], integer[])
RETURNS boolean LANGUAGE sql AS 'SELECT true';
CREATE OPERATOR public.@ (LEFTARG = integer[], RIGHTARG = integer[], FUNCTION = public.drop_operator__int4);
DROP OPERATOR @ (_int4, _int4);
CREATE OPERATOR public.~ (LEFTARG = integer[], RIGHTARG = integer[], FUNCTION = public.drop_operator__int4);
DROP OPERATOR ~ (_int4, _int4);
