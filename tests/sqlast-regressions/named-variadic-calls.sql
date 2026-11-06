CREATE FUNCTION f(a integer, b text DEFAULT 'default', c integer DEFAULT 3)
RETURNS integer LANGUAGE SQL STRICT AS 'SELECT a + c';
CREATE FUNCTION v(a integer, VARIADIC xs integer[])
RETURNS integer LANGUAGE SQL STRICT AS 'SELECT a';
CREATE FUNCTION arr(xs integer[]) RETURNS integer LANGUAGE SQL AS 'SELECT 1';
CREATE FUNCTION scalar(a integer) RETURNS integer LANGUAGE SQL AS 'SELECT a';
CREATE FUNCTION poly(value anyelement) RETURNS anyelement LANGUAGE SQL AS 'SELECT value';
CREATE FUNCTION polyarray(VARIADIC xs anyarray) RETURNS anyelement LANGUAGE SQL AS 'SELECT xs[1]';
CREATE FUNCTION compatible(a anycompatible, b anycompatible) RETURNS anycompatible
LANGUAGE SQL AS 'SELECT a';
CREATE FUNCTION pick(a integer) RETURNS integer LANGUAGE SQL AS 'SELECT a';
CREATE FUNCTION pick(b text) RETURNS text LANGUAGE SQL AS 'SELECT b';
CREATE FUNCTION collide(a integer, b integer DEFAULT 1) RETURNS integer LANGUAGE SQL AS 'SELECT a';
CREATE FUNCTION collide(a integer, b integer DEFAULT 1, c integer DEFAULT 2) RETURNS integer LANGUAGE SQL AS 'SELECT a';
CREATE FUNCTION output(a integer, OUT x integer, OUT y text) LANGUAGE SQL AS 'SELECT a, a::text';
CREATE FUNCTION inout_value(INOUT a integer) LANGUAGE SQL AS 'SELECT a';
CREATE FUNCTION quoted("Arg" integer) RETURNS integer LANGUAGE SQL AS 'SELECT "Arg"';
CREATE FUNCTION polyout(value anyelement, OUT x anyelement, OUT y text)
LANGUAGE SQL AS 'SELECT value, ''label''::text';
CREATE FUNCTION uuid_value(value uuid) RETURNS uuid LANGUAGE SQL AS 'SELECT value';
CREATE FUNCTION uuid_array(value uuid[]) RETURNS uuid[] LANGUAGE SQL AS 'SELECT value';
CREATE FUNCTION compatible_array(a anycompatiblearray, b anycompatiblearray)
RETURNS anycompatiblearray LANGUAGE SQL AS 'SELECT a';
CREATE FUNCTION compatible_variadic(VARIADIC xs anycompatiblearray)
RETURNS anycompatiblearray LANGUAGE SQL AS 'SELECT xs';
CREATE FUNCTION compatible_nonarray(a anycompatiblenonarray, b anycompatible)
RETURNS anycompatible LANGUAGE SQL AS 'SELECT a';
