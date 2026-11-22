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

CREATE FUNCTION public.operator_restrict(internal, oid, internal, integer)
RETURNS double precision LANGUAGE internal AS 'contsel';

CREATE FUNCTION public.operator_join(internal, oid, internal, smallint, internal)
RETURNS double precision LANGUAGE internal AS 'contjoinsel';

CREATE OPERATOR public.@#@# (
    LEFTARG = integer,
    RIGHTARG = integer,
    FUNCTION = pg_catalog.int4eq,
    COMMUTATOR = '@#@#',
    RESTRICT = public.operator_restrict,
    JOIN = public.operator_join,
    HASHES = 'off',
    MERGES
);
