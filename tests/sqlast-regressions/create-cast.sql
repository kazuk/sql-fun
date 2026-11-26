CREATE TYPE public.cast_source AS ENUM ('one');
CREATE TYPE public.cast_assignment AS ENUM ('one');
CREATE TYPE public.cast_implicit AS ENUM ('one');
CREATE TYPE public.cast_explicit AS ENUM ('one');

CREATE FUNCTION public.to_explicit(public.cast_source, integer, boolean)
RETURNS public.cast_explicit LANGUAGE sql STRICT
AS 'SELECT $1::text::public.cast_explicit';

CREATE CAST (public.cast_source AS public.cast_explicit)
WITH FUNCTION public.to_explicit(public.cast_source, integer, boolean);

CREATE CAST (public.cast_source AS public.cast_assignment) WITH INOUT AS ASSIGNMENT;
CREATE CAST (public.cast_source AS public.cast_implicit) WITH INOUT AS IMPLICIT;
CREATE CAST (oid AS float4) WITHOUT FUNCTION AS IMPLICIT;

CREATE TABLE public.cast_inputs (value public.cast_source NOT NULL);
CREATE TABLE public.cast_outputs (value public.cast_assignment);

CREATE FUNCTION public.consume_cast(public.cast_implicit) RETURNS integer
LANGUAGE sql AS 'SELECT 1';

CREATE FUNCTION public.text_to_source(text) RETURNS public.cast_source
LANGUAGE sql STRICT AS 'SELECT $1::public.cast_source';
CREATE CAST (varchar AS public.cast_source)
WITH FUNCTION public.text_to_source(text);

CREATE FUNCTION public.float_to_source(float4) RETURNS public.cast_source
LANGUAGE sql STRICT AS 'SELECT ''one''::public.cast_source';
CREATE CAST (oid AS public.cast_source)
WITH FUNCTION public.float_to_source(float4);

CREATE FUNCTION public.consume_assignment(public.cast_assignment) RETURNS integer
LANGUAGE sql AS 'SELECT 1';

CREATE TYPE public.cast_text;
CREATE FUNCTION public.cast_text_in(cstring) RETURNS public.cast_text
LANGUAGE internal IMMUTABLE STRICT AS 'textin';
CREATE FUNCTION public.cast_text_out(public.cast_text) RETURNS cstring
LANGUAGE internal IMMUTABLE STRICT AS 'textout';
CREATE TYPE public.cast_text (
    INPUT = public.cast_text_in,
    OUTPUT = public.cast_text_out,
    INTERNALLENGTH = variable,
    STORAGE = extended
);
CREATE CAST (public.cast_text AS text) WITHOUT FUNCTION AS IMPLICIT;

CREATE FUNCTION public.bool_to_implicit(boolean) RETURNS public.cast_implicit
LANGUAGE sql AS 'SELECT ''one''::public.cast_implicit';
CREATE CAST (boolean AS public.cast_implicit)
WITH FUNCTION public.bool_to_implicit(boolean) AS IMPLICIT;
CREATE FUNCTION public.consume_source(boolean) RETURNS integer
LANGUAGE sql AS 'SELECT 1';

CREATE DOMAIN public.cast_domain AS public.cast_source;

-- These declarations exercise analysis only. PostgreSQL checks runtime layout.
CREATE CAST (uuid AS macaddr8) WITHOUT FUNCTION;

CREATE TYPE public.binary_assignment AS ENUM ('one');
CREATE TYPE public.binary_implicit AS ENUM ('one');
CREATE CAST (public.cast_source AS public.binary_assignment)
WITHOUT FUNCTION AS ASSIGNMENT;
CREATE CAST (public.cast_source AS public.binary_implicit)
WITHOUT FUNCTION AS IMPLICIT;

CREATE TABLE public.binary_outputs (value public.binary_assignment);
CREATE FUNCTION public.consume_binary(public.binary_assignment) RETURNS text
LANGUAGE sql AS 'SELECT ''assignment''';
CREATE FUNCTION public.consume_binary(public.binary_implicit) RETURNS integer
LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION public.consume_binary_assignment(public.binary_assignment) RETURNS integer
LANGUAGE sql AS 'SELECT 2';
