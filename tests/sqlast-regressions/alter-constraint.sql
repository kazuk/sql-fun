CREATE TABLE constraint_parent (id integer CONSTRAINT parent_pk PRIMARY KEY);
CREATE TABLE constraint_child (
    id integer CONSTRAINT inline_fk REFERENCES constraint_parent(id),
    CONSTRAINT child_fk FOREIGN KEY (id) REFERENCES constraint_parent(id),
    CONSTRAINT child_check CHECK (id >= 0),
    CONSTRAINT child_unique UNIQUE (id)
);
ALTER TABLE constraint_child ADD CONSTRAINT pending_check CHECK (id < 100) NOT VALID;
