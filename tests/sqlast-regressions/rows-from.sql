CREATE TYPE public.rows508_pair AS (id integer, label text);
CREATE FUNCTION public.rows508_srf(n integer) RETURNS SETOF integer LANGUAGE SQL AS 'SELECT generate_series(1,n)';
