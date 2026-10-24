CREATE TABLE public.items(id integer, label text);
CREATE TABLE public.arrays(a integer[]);
CREATE TABLE public.identity_items(id integer GENERATED ALWAYS AS IDENTITY);
CREATE TABLE public.generated_items(g integer GENERATED ALWAYS AS (NULL::integer) STORED);
