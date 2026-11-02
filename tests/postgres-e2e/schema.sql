CREATE TABLE e2e_items (
    id bigint PRIMARY KEY,
    name text NOT NULL
);

CREATE VIEW e2e_item_view AS SELECT id, name FROM e2e_items;
