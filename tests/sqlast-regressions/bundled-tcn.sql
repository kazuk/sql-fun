CREATE TABLE items (id integer);
CREATE TRIGGER notify_item AFTER INSERT ON items
FOR EACH ROW EXECUTE FUNCTION triggered_change_notification();
