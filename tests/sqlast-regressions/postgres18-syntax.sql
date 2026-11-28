CREATE TABLE public.migration_rows (id integer PRIMARY KEY);
CREATE TABLE public.nullable_rows (id integer);
CREATE TABLE public.name_holder (
    id integer CONSTRAINT original_key_rows_id_not_null CHECK (id > 0)
);
CREATE TABLE public.original_key_rows (id integer, PRIMARY KEY (id));
ALTER TABLE public.original_key_rows RENAME TO renamed_key_rows;
CREATE TABLE public.late_key_rows (id integer);
ALTER TABLE public.late_key_rows ADD CONSTRAINT late_key_pk PRIMARY KEY (id);
