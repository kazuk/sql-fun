CREATE SCHEMA a;
CREATE SCHEMA b;
CREATE SCHEMA "a.b";

CREATE TABLE a.t(id integer NOT NULL);
CREATE TABLE b.t(id bigint);
CREATE TABLE a.u(id integer);
CREATE TABLE "a.b"."t.x"(id integer);

CREATE VIEW a.v AS SELECT id FROM a.t;
