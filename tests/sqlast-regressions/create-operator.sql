CREATE FUNCTION public.operator_text(text, text) RETURNS text
LANGUAGE sql STRICT AS 'SELECT $1 || $2';

CREATE OPERATOR public.%% (
    LEFTARG = text,
    RIGHTARG = text,
    FUNCTION = public.operator_text
);

CREATE OPERATOR public.%% (
    LEFTARG = integer,
    RIGHTARG = integer,
    PROCEDURE = pg_catalog.int4pl
);

CREATE OPERATOR public.%% (
    RIGHTARG = integer,
    FUNCTION = pg_catalog.int4um
);

CREATE OPERATOR public.@#@# (
    LEFTARG = integer,
    RIGHTARG = integer,
    FUNCTION = pg_catalog.int4eq,
    COMMUTATOR = '@#@#',
    RESTRICT = pg_catalog.contsel,
    JOIN = contjoinsel,
    HASHES = 'off',
    MERGES
);

ALTER OPERATOR public.@#@# (integer, integer) SET (JOIN = "Planner"."JoinFn");

-- Supply the prerequisite declarations for the audited extension ALTERs.
CREATE DOMAIN public.cube AS integer;
CREATE DOMAIN public.seg AS integer;
CREATE FUNCTION public.cube_cmp(cube, cube) RETURNS boolean
LANGUAGE sql AS 'SELECT true';
CREATE FUNCTION public.seg_cmp(seg, seg) RETURNS boolean
LANGUAGE sql AS 'SELECT true';
CREATE FUNCTION public.trgm_cmp(text, text) RETURNS boolean
LANGUAGE sql AS 'SELECT true';
CREATE FUNCTION public.prefix_predicate(integer) RETURNS boolean
LANGUAGE sql AS 'SELECT true';
CREATE OPERATOR public.?# (RIGHTARG=integer, FUNCTION=public.prefix_predicate);
CREATE OPERATOR public.?@ (LEFTARG=integer, RIGHTARG=integer, FUNCTION=pg_catalog.int4ne);
CREATE FUNCTION public.scalarlesel(internal, oid, internal, integer)
RETURNS double precision LANGUAGE internal AS 'scalarlesel';
CREATE FUNCTION public.scalargesel(internal, oid, internal, integer)
RETURNS double precision LANGUAGE internal AS 'scalargesel';
CREATE FUNCTION public.matchingsel(internal, oid, internal, integer)
RETURNS double precision LANGUAGE internal AS 'matchingsel';
CREATE FUNCTION public.scalarlejoinsel(internal, oid, internal, smallint, internal)
RETURNS double precision LANGUAGE internal AS 'scalarlejoinsel';
CREATE FUNCTION public.scalargejoinsel(internal, oid, internal, smallint, internal)
RETURNS double precision LANGUAGE internal AS 'scalargejoinsel';
CREATE FUNCTION public.matchingjoinsel(internal, oid, internal, smallint, internal)
RETURNS double precision LANGUAGE internal AS 'matchingjoinsel';
CREATE OPERATOR public.<= (LEFTARG=cube, RIGHTARG=cube, FUNCTION=public.cube_cmp);
CREATE OPERATOR public.>= (LEFTARG=cube, RIGHTARG=cube, FUNCTION=public.cube_cmp);
CREATE OPERATOR public.<= (LEFTARG=seg, RIGHTARG=seg, FUNCTION=public.seg_cmp);
CREATE OPERATOR public.>= (LEFTARG=seg, RIGHTARG=seg, FUNCTION=public.seg_cmp);
CREATE OPERATOR public.% (LEFTARG=text, RIGHTARG=text, FUNCTION=public.trgm_cmp);
CREATE OPERATOR public.<% (LEFTARG=text, RIGHTARG=text, FUNCTION=public.trgm_cmp);
CREATE OPERATOR public.%> (LEFTARG=text, RIGHTARG=text, FUNCTION=public.trgm_cmp);
CREATE OPERATOR public.<<% (LEFTARG=text, RIGHTARG=text, FUNCTION=public.trgm_cmp);
CREATE OPERATOR public.%>> (LEFTARG=text, RIGHTARG=text, FUNCTION=public.trgm_cmp);
