DO LANGUAGE plpgsql $$
BEGIN
    CREATE TABLE public.do_extension_hidden (id integer);
END
$$;

CREATE TABLE public.do_extension_visible (id integer NOT NULL);
