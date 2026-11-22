CREATE TYPE public.base_value;
CREATE FUNCTION public.base_value_in(cstring) RETURNS public.base_value
  LANGUAGE internal IMMUTABLE STRICT AS 'textin';
CREATE FUNCTION public.base_value_out(public.base_value) RETURNS cstring
  LANGUAGE internal IMMUTABLE STRICT AS 'textout';
CREATE TYPE public.base_value (
  INPUT = public.base_value_in,
  OUTPUT = public.base_value_out,
  INTERNALLENGTH = VARIABLE,
  STORAGE = extended,
  CATEGORY = 'SS',
  COLLATABLE = true
);
CREATE TABLE public.base_values (
  item public.base_value COLLATE "C",
  items public.base_value[]
);

CREATE FUNCTION public.implicit_value_in(cstring) RETURNS public.implicit_value
  LANGUAGE internal IMMUTABLE STRICT AS 'textin';
CREATE FUNCTION public.implicit_value_out(public.implicit_value) RETURNS cstring
  LANGUAGE internal IMMUTABLE STRICT AS 'textout';
CREATE TYPE public.implicit_value (
  INPUT = public.implicit_value_in,
  OUTPUT = public.implicit_value_out,
  INTERNALLENGTH = VARIABLE,
  STORAGE = extended
);
CREATE TABLE public.implicit_values (
  item public.implicit_value,
  items public.implicit_value[]
);
