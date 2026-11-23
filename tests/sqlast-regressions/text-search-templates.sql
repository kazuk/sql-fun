CREATE SCHEMA "Search";
CREATE FUNCTION "Search"."Lexize"(internal, internal, internal, internal)
  RETURNS internal AS 'callback_fixture' LANGUAGE C;
CREATE FUNCTION public.init_callback(internal)
  RETURNS internal AS 'callback_fixture' LANGUAGE C;
CREATE FUNCTION public.bad_result(internal, internal, internal, internal)
  RETURNS int4 AS 'callback_fixture' LANGUAGE C;
CREATE TEXT SEARCH TEMPLATE public.existing (LEXIZE = "Search"."Lexize");
