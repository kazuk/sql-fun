-- Issue #768: independent view rule/trigger precedence scenarios.
CREATE TABLE public.items(id integer PRIMARY KEY);
CREATE TABLE public.audit(id integer);
CREATE FUNCTION public.noop() RETURNS trigger LANGUAGE plpgsql
  AS $$BEGIN RETURN NEW; END$$;

CREATE VIEW public.v_instead_action AS SELECT id FROM public.items;
CREATE RULE r AS ON INSERT TO public.v_instead_action DO INSTEAD INSERT INTO public.audit VALUES(NEW.id);

CREATE VIEW public.v_conditional_action_trigger AS SELECT id FROM public.items;
CREATE RULE r AS ON INSERT TO public.v_conditional_action_trigger WHERE NEW.id>0 DO INSTEAD INSERT INTO public.audit VALUES(NEW.id);
CREATE TRIGGER ti INSTEAD OF INSERT ON public.v_conditional_action_trigger
  FOR EACH ROW EXECUTE FUNCTION public.noop();

CREATE VIEW public.v_mixed_rules AS SELECT id FROM public.items;
CREATE RULE r AS ON INSERT TO public.v_mixed_rules DO INSTEAD NOTHING;
CREATE RULE a AS ON INSERT TO public.v_mixed_rules DO ALSO INSERT INTO public.audit VALUES(NEW.id);
CREATE TRIGGER ti INSTEAD OF INSERT ON public.v_mixed_rules
  FOR EACH ROW EXECUTE FUNCTION public.noop();

CREATE VIEW public.v_update_nothing_instead AS SELECT id FROM public.items;
CREATE RULE r AS ON INSERT TO public.v_update_nothing_instead DO INSTEAD NOTHING;
CREATE RULE u AS ON UPDATE TO public.v_update_nothing_instead DO INSTEAD NOTHING;

CREATE VIEW public.v_update_nothing_trigger AS SELECT id FROM public.items;
CREATE RULE u AS ON UPDATE TO public.v_update_nothing_trigger DO INSTEAD NOTHING;
CREATE TRIGGER ti INSTEAD OF INSERT ON public.v_update_nothing_trigger
  FOR EACH ROW EXECUTE FUNCTION public.noop();

CREATE VIEW public.v_also_nothing_trigger AS SELECT id FROM public.items;
CREATE RULE r AS ON INSERT TO public.v_also_nothing_trigger DO ALSO NOTHING;
CREATE TRIGGER ti INSTEAD OF INSERT ON public.v_also_nothing_trigger
  FOR EACH ROW EXECUTE FUNCTION public.noop();
