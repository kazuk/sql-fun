CREATE TABLE public.left_items (id integer NOT NULL, left_value text);
CREATE TABLE public.right_items (id bigint NOT NULL, right_value text);
CREATE VIEW public.joined_items AS
SELECT * FROM public.left_items l FULL JOIN public.right_items r USING (id);
