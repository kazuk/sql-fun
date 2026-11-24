DO $$
BEGIN
    DROP TABLE public.do_extension_visible;
    CREATE TABLE public.do_project_hidden (id integer);
END
$$;

CREATE VIEW public.do_project_visible AS
SELECT id FROM public.do_extension_visible;
