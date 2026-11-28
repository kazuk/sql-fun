CREATE TABLE items (id integer);
CREATE SEQUENCE item_seq;
CREATE TRIGGER assign_id BEFORE INSERT ON items
FOR EACH ROW EXECUTE FUNCTION autoinc(id, item_seq);
