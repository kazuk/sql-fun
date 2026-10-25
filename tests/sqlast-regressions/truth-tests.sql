CREATE TABLE flags(nullable boolean, required boolean NOT NULL);
CREATE DOMAIN flag AS boolean;
CREATE TABLE domain_flags(value flag);
