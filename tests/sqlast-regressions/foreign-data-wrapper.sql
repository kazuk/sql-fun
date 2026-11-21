CREATE SCHEMA callbacks;
CREATE FUNCTION pg_catalog.validator(text) RETURNS text LANGUAGE sql AS 'SELECT $1';
CREATE FUNCTION public.validator(text[], oid) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE PROCEDURE public.proc_validator(text[], oid) LANGUAGE sql AS 'SELECT 1';
CREATE FOREIGN DATA WRAPPER existing_wrapper;
CREATE FUNCTION callbacks.wrapper_handler() RETURNS fdw_handler LANGUAGE C AS 'wrapper_handler';
CREATE PROCEDURE pg_catalog.validator_shadow(text[], oid) LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION public.validator_shadow(text[], oid) RETURNS integer LANGUAGE sql AS 'SELECT 1';
