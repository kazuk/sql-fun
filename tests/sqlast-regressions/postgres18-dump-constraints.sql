--
-- PostgreSQL database dump
--

\restrict sqlfunpg18constraints

-- Dumped from database version 18.1 (Debian 18.1-1.pgdg13+2)
-- Dumped by pg_dump version 18.7 (Ubuntu 18.7-1.pgdg24.04+2)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: pg18_constraints; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA pg18_constraints;

CREATE TYPE pg18_constraints.fake_range AS ENUM ('x');


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: child; Type: TABLE; Schema: pg18_constraints; Owner: -
--

CREATE TABLE pg18_constraints.child (
    a daterange,
    b daterange
);


--
-- Name: parent; Type: TABLE; Schema: pg18_constraints; Owner: -
--

CREATE TABLE pg18_constraints.parent (
    a daterange NOT NULL,
    b daterange NOT NULL
);


--
-- Name: t; Type: TABLE; Schema: pg18_constraints; Owner: -
--

CREATE TABLE pg18_constraints.t (
    a integer CONSTRAINT nn NOT NULL NO INHERIT,
    b integer
);


--
-- Name: t bn; Type: CONSTRAINT; Schema: pg18_constraints; Owner: -
--

ALTER TABLE pg18_constraints.t
    ADD CONSTRAINT bn NOT NULL b NOT VALID;


--
-- Name: t ck; Type: CHECK CONSTRAINT; Schema: pg18_constraints; Owner: -
--

ALTER TABLE pg18_constraints.t
    ADD CONSTRAINT ck CHECK ((b > 0)) NOT ENFORCED;


--
-- Name: parent parent_b_a_key; Type: CONSTRAINT; Schema: pg18_constraints; Owner: -
--

ALTER TABLE ONLY pg18_constraints.parent
    ADD CONSTRAINT parent_b_a_key UNIQUE (b, a WITHOUT OVERLAPS);


--
-- Name: parent parent_pkey; Type: CONSTRAINT; Schema: pg18_constraints; Owner: -
--

ALTER TABLE ONLY pg18_constraints.parent
    ADD CONSTRAINT parent_pkey PRIMARY KEY (a, b WITHOUT OVERLAPS);


--
-- Name: child child_a_b_fkey; Type: FK CONSTRAINT; Schema: pg18_constraints; Owner: -
--

ALTER TABLE ONLY pg18_constraints.child
    ADD CONSTRAINT child_a_b_fkey FOREIGN KEY (a, PERIOD b) REFERENCES pg18_constraints.parent(a, PERIOD b);


--
-- PostgreSQL database dump complete
--

\unrestrict sqlfunpg18constraints
