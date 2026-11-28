CREATE TABLE items (id integer);
CREATE TABLE parents (id integer PRIMARY KEY);
CREATE TRIGGER verify_parent BEFORE INSERT ON items
FOR EACH ROW EXECUTE FUNCTION check_primary_key(id, parents, id);
