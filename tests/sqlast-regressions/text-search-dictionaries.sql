CREATE SCHEMA "Search";
CREATE FUNCTION public.lexize(internal, internal, internal, internal)
  RETURNS internal AS 'callback_fixture' LANGUAGE C;
CREATE FUNCTION public.init_callback(internal)
  RETURNS internal AS 'callback_fixture' LANGUAGE C;
CREATE TEXT SEARCH TEMPLATE "Search"."Opts"
  (LEXIZE = lexize, INIT = init_callback);
CREATE TEXT SEARCH TEMPLATE public.no_options (LEXIZE = lexize);
CREATE TEXT SEARCH DICTIONARY public.existing (TEMPLATE = no_options);
