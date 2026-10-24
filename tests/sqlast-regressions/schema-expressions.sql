CREATE DOMAIN public.amount AS integer;
CREATE DOMAIN public.amount_array AS integer[];
CREATE DOMAIN public.nested_amount_array AS public.amount_array;
CREATE TYPE public.mood AS ENUM ('low', 'high');
CREATE TYPE public.pair AS (n integer);
CREATE TABLE public.items (n integer);
