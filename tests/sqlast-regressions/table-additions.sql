CREATE TABLE addition_items(a integer);
ALTER TABLE addition_items ADD COLUMN b integer NOT NULL DEFAULT 1, ADD COLUMN c text;
CREATE VIEW addition_old_view AS SELECT * FROM addition_items;
ALTER TABLE addition_items ADD COLUMN d bigint;
CREATE TABLE addition_parent(id integer PRIMARY KEY);
CREATE TABLE addition_serial(a integer);
ALTER TABLE addition_serial ADD COLUMN id serial;
