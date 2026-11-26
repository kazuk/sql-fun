CREATE FUNCTION public.parallel_probe() RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION public.parallel_probe(text) RETURNS text LANGUAGE sql AS 'SELECT $1';

ALTER FUNCTION public.parallel_probe() PARALLEL SAFE;
ALTER FUNCTION public.parallel_probe(text) PARALLEL RESTRICTED;

CREATE VIEW public.parallel_result AS SELECT public.parallel_probe() AS value;

CREATE FUNCTION public.array_by_catalog_name(_text)
RETURNS pg_catalog._text LANGUAGE sql AS 'SELECT $1';
ALTER FUNCTION public.array_by_catalog_name(text[]) PARALLEL SAFE;

CREATE TYPE public.scalar_text AS ENUM ('scalar');
CREATE FUNCTION public.scalar_signature_probe(public.scalar_text)
RETURNS integer LANGUAGE sql AS 'SELECT 1';
ALTER FUNCTION public.scalar_signature_probe(public.scalar_text) PARALLEL SAFE;
ALTER FUNCTION public.array_by_catalog_name(_text) PARALLEL SAFE;

CREATE TYPE public.array_fixture_element AS ENUM ('example');
CREATE TABLE public.array_fixture (value public._array_fixture_element);
CREATE FUNCTION public.schema_array_probe(public._array_fixture_element)
RETURNS public.array_fixture_element[] LANGUAGE sql AS 'SELECT $1';
ALTER FUNCTION public.schema_array_probe(public.array_fixture_element[]) PARALLEL SAFE;

CREATE FUNCTION public.builtin_array_operator(integer[], integer[]) RETURNS integer[]
LANGUAGE sql AS 'SELECT $1 || $2';
CREATE OPERATOR public.?#? (
  LEFTARG = pg_catalog._int4, RIGHTARG = integer[], FUNCTION = public.builtin_array_operator
);
ALTER TABLE public.array_fixture RENAME COLUMN value TO _value;
