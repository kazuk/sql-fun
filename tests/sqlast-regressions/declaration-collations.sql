CREATE DOMAIN public.collation_text AS text COLLATE "C";
CREATE DOMAIN public.collation_nested AS public.collation_text;
CREATE DOMAIN public.collation_integer AS integer;
CREATE TYPE public.collation_enum AS ENUM ('one');
CREATE TYPE public.collation_composite AS (value text COLLATE "C");
CREATE TABLE public.collation_row(value text COLLATE "C");
CREATE TABLE public.collation_columns (
  scalar text COLLATE "C",
  items text[] COLLATE pg_catalog."C",
  nested public.collation_nested COLLATE "C"
);
CREATE TYPE public."composite.with.dot" AS ("Mixed.Field" varchar(12) COLLATE pg_catalog."C");
