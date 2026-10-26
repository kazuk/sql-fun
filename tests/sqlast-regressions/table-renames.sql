CREATE TABLE public.rename_before (id integer NOT NULL, label text);
CREATE VIEW public.rename_preserved AS SELECT id, label FROM public.rename_before;
ALTER TABLE public.rename_before RENAME TO rename_after;
CREATE TABLE public.rename_before (replacement integer);
CREATE TABLE public.rename_repeated (id integer NOT NULL);
ALTER TABLE public.rename_repeated RENAME TO rename_middle;
ALTER TABLE public.rename_middle RENAME TO rename_final;
