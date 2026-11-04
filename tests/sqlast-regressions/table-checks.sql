CREATE DOMAIN check_flag AS boolean;
CREATE TABLE check_existing(a integer, b integer, CONSTRAINT existing CHECK(a < b));
