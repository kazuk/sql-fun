CREATE TYPE control AS (
    system_identifier bigint,
    pg_control_version integer,
    catalog_version_no integer,
    pg_control_last_modified timestamptz
);
CREATE FUNCTION f(value control) RETURNS int LANGUAGE sql AS 'SELECT 1';
