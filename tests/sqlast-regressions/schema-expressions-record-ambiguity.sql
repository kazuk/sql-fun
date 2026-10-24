CREATE FUNCTION g(value record, other integer) RETURNS integer LANGUAGE plpgsql AS 'BEGIN RETURN 1; END';
CREATE FUNCTION g(value record, other bigint) RETURNS bigint LANGUAGE plpgsql AS 'BEGIN RETURN 1; END';
CREATE FUNCTION ga(value anyarray, other integer) RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE FUNCTION ga(value anyarray, other bigint) RETURNS bigint LANGUAGE sql AS 'SELECT 1::bigint';
