CREATE TABLE public.column_rename_base (old integer NOT NULL, payload text);
CREATE VIEW public.column_rename_view AS SELECT old, payload FROM public.column_rename_base;
ALTER TABLE public.column_rename_base RENAME COLUMN old TO renamed;
CREATE TABLE public.column_rename_repeated (a integer NOT NULL, b text);
ALTER TABLE public.column_rename_repeated RENAME COLUMN a TO intermediate;
ALTER TABLE public.column_rename_repeated RENAME COLUMN b TO a;
ALTER TABLE public.column_rename_repeated RENAME COLUMN intermediate TO c;
CREATE TABLE public.column_rename_quoted ("Old" integer);
ALTER TABLE public.column_rename_quoted RENAME COLUMN "Old" TO "New";
