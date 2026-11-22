CREATE ACCESS METHOD project_index TYPE INDEX HANDLER declared_handler;
CREATE OPERATOR FAMILY project_family USING btree;
CREATE OPERATOR CLASS project_ops FOR TYPE integer USING btree FAMILY project_family AS
    OPERATOR 3 =,
    FUNCTION 1 pg_catalog.btint4cmp(integer, integer);

CREATE TABLE operator_class_items (id integer PRIMARY KEY);
