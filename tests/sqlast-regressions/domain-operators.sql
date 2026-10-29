CREATE DOMAIN public.distinct_number AS integer;
CREATE DOMAIN public.nested_number AS public.distinct_number;
CREATE DOMAIN public.number_array AS integer[];
CREATE TABLE public.distinct_values (
    d public.distinct_number,
    required public.distinct_number NOT NULL,
    nested public.nested_number
);
CREATE FUNCTION public.domain_only(public.distinct_number, public.distinct_number)
RETURNS boolean LANGUAGE sql IMMUTABLE STRICT AS 'SELECT $1::integer = $2::integer';
