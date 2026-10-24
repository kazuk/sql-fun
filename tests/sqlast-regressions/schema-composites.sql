CREATE TYPE public.singleton AS (n integer);
CREATE TYPE public.pair AS (n integer, m integer);
CREATE TYPE public.container AS (item public.singleton);

CREATE FUNCTION public.take_singleton(public.singleton) RETURNS integer
  LANGUAGE sql IMMUTABLE AS 'SELECT 1';
CREATE FUNCTION public.take_pair(public.pair) RETURNS integer
  LANGUAGE sql IMMUTABLE AS 'SELECT 1';
CREATE FUNCTION public.take_container(public.container) RETURNS integer
  LANGUAGE sql IMMUTABLE AS 'SELECT 1';
