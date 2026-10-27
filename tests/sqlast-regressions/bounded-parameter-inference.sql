CREATE TYPE inference_row AS (id integer);
CREATE TYPE inference_color AS ENUM ('red', 'green');
CREATE DOMAIN inference_amount AS integer;
CREATE TABLE inference_values (
    bytes bytea,
    identifier uuid,
    custom_arrays inference_row[]
);
