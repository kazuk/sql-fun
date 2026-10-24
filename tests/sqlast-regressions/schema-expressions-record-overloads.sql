CREATE TYPE control AS (
    system_identifier bigint,
    pg_control_version integer,
    catalog_version_no integer,
    pg_control_last_modified timestamptz
);
CREATE FUNCTION f(value record) RETURNS text LANGUAGE plpgsql AS 'BEGIN RETURN NULL; END';
CREATE FUNCTION f(value control) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION fa(value anyarray) RETURNS text LANGUAGE sql AS 'SELECT NULL::text';
CREATE FUNCTION fa(value control[]) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION fp(value record) RETURNS text LANGUAGE plpgsql AS 'BEGIN RETURN NULL; END';
CREATE FUNCTION fp(value anyelement) RETURNS integer LANGUAGE sql AS 'SELECT 1';
