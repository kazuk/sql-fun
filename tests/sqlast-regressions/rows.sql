CREATE TYPE pair AS (left_value integer, right_value integer);
CREATE TABLE items(id integer NOT NULL, label text);
CREATE TYPE wide_pair AS (n bigint, flag integer);
CREATE TYPE nested_pair AS (inner_value pair, flag boolean);
