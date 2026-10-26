CREATE TYPE public.status AS ENUM ('ready');
CREATE TYPE other_status AS ENUM ('ready');
CREATE TABLE public.enum_items (
  status public.status,
  other other_status
);
CREATE FUNCTION public.enum_identity(public.status) RETURNS public.status
  LANGUAGE sql IMMUTABLE AS 'SELECT $1';
