CREATE DOMAIN public.scalar_d AS integer;
CREATE TYPE public.leaf AS (n public.scalar_d);
CREATE DOMAIN public.leaf_d AS public.leaf;
CREATE DOMAIN public.leaf_dd AS public.leaf_d;
CREATE TYPE public.outer_row AS (item public.leaf_dd);
CREATE DOMAIN public.outer_d AS public.outer_row;
CREATE TABLE public.domain_table (n public.scalar_d);
CREATE DOMAIN public.table_d AS public.domain_table;
CREATE DOMAIN public.arr_d AS public.leaf[];
