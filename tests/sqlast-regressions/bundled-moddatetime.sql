CREATE TABLE items (modified timestamp);
CREATE TRIGGER stamp_item BEFORE UPDATE ON items
FOR EACH ROW EXECUTE FUNCTION moddatetime(modified);
