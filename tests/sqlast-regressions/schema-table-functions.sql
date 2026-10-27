CREATE TYPE public.pair AS (a integer, b text);

CREATE FUNCTION public.record_fun(src text) RETURNS record
  LANGUAGE sql IMMUTABLE AS 'SELECT 1, ''x''';
CREATE FUNCTION public.out_fun(OUT a integer, OUT b text) RETURNS record
  LANGUAGE sql IMMUTABLE AS 'SELECT 1, ''x''';
CREATE FUNCTION public.pair_fun() RETURNS public.pair
  LANGUAGE sql IMMUTABLE AS 'SELECT ROW(1, ''x'')::public.pair';
CREATE FUNCTION public.identity(integer) RETURNS integer
  LANGUAGE sql IMMUTABLE AS 'SELECT $1';
