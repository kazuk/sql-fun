CREATE DOMAIN check_flag AS boolean;
CREATE TABLE check_existing(a integer, b integer, CONSTRAINT existing CHECK(a < b));
CREATE TABLE check_parent(id integer PRIMARY KEY);
