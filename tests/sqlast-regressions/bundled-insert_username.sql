CREATE TABLE items (author text);
CREATE TRIGGER set_author BEFORE INSERT ON items
FOR EACH ROW EXECUTE FUNCTION insert_username(author);
