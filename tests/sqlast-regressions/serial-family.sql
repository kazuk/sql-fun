CREATE TABLE serial_family (
    a smallserial,
    b serial2,
    c serial,
    d serial4,
    e bigserial,
    f serial8,
    quoted "serial"
);
CREATE TABLE serial_keys(id serial PRIMARY KEY, other bigserial UNIQUE);
CREATE SEQUENCE serial_collision_id_seq;
CREATE TABLE serial_collision(id serial);
CREATE TABLE "Serial Table"("Serial Column" bigserial);
