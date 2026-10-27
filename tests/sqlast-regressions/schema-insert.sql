CREATE TABLE public.items(id integer NOT NULL, label text);
CREATE TABLE public.flags(id integer, flag boolean);
CREATE TABLE public.arrays(a integer[]);
CREATE TABLE public.identity_items(id integer GENERATED ALWAYS AS IDENTITY);
CREATE TABLE public.generated_items(id integer, g integer GENERATED ALWAYS AS (NULL::integer) STORED);
