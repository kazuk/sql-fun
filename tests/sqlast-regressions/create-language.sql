CREATE SCHEMA callbacks;
CREATE FUNCTION callbacks.inline_handler(internal) RETURNS integer LANGUAGE c AS 'symbol';
CREATE FUNCTION public.validator(integer) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION callbacks.validator(oid) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION pg_catalog.validator_shadow(integer) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION public.validator_shadow(oid) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE LANGUAGE existing_language HANDLER pg_catalog.plpgsql_call_handler;
