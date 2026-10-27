use super::tests::assert_postgres_structure;
use super::{SqlEmit, SqlWriter};
use crate::ast::{Dialect, DropTable, Name};

#[rstest::rstest]
#[case("SET work_mem TO '64MB'")]
#[case("SET LOCAL statement_timeout FROM CURRENT")]
#[case("SET search_path TO public, pg_catalog")]
#[case(r#"SET "foo.bar" TO 'custom'"#)]
#[case("SET custom.setting TO 'custom'")]
#[case("SET TIME ZONE DEFAULT")]
#[case("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE, READ ONLY, DEFERRABLE")]
#[case("SET LOCAL TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ WRITE")]
#[case(
    "SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL READ COMMITTED, READ WRITE, NOT DEFERRABLE"
)]
#[case("SET LOCAL SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL SERIALIZABLE")]
#[case("SET TRANSACTION SNAPSHOT '00000003-0000001B-1'")]
#[case("SET LOCAL TRANSACTION SNAPSHOT '00000003-0000001B-1'")]
#[case("RESET work_mem")]
#[case("RESET ALL")]
#[case("CREATE EXTENSION IF NOT EXISTS hstore WITH SCHEMA public VERSION '1.4'")]
#[case(
    "CREATE SEQUENCE IF NOT EXISTS app.s AS bigint INCREMENT BY 2 MINVALUE 1 MAXVALUE 100 START WITH 3 CACHE 4 CYCLE OWNED BY app.t.id"
)]
#[case(
    "ALTER SEQUENCE IF EXISTS app.s AS smallint INCREMENT BY 3 NO MINVALUE NO MAXVALUE RESTART WITH 7 CACHE 2 NO CYCLE OWNED BY NONE"
)]
#[case("CREATE TEMPORARY SEQUENCE s")]
#[case("CREATE TYPE app.mood AS ENUM ('sad', 'ok', 'happy')")]
#[case("DROP TABLE IF EXISTS app.t, archive.t CASCADE")]
#[case("INSERT INTO app.items AS target (id) VALUES (1)")]
#[case(
    "CREATE OR REPLACE TEMP VIEW app.v (a, b) WITH (security_barrier = true) AS SELECT 1, 2 WITH LOCAL CHECK OPTION"
)]
#[case(
    "CREATE MATERIALIZED VIEW app.mv (a) USING heap WITH (fillfactor = 70) AS SELECT 1 WITH NO DATA"
)]
#[case("COMMENT ON COLUMN app.t.c IS 'column comment'")]
#[case("COMMENT ON TYPE app.status IS 'type comment'")]
#[case("COMMENT ON DOMAIN app.domain_name IS 'domain comment'")]
#[case("COMMENT ON CONSTRAINT ck ON app.t IS 'constraint comment'")]
#[case("COMMENT ON CONSTRAINT ck ON DOMAIN app.domain_name IS 'domain constraint comment'")]
#[case("COMMENT ON CAST (integer AS text) IS 'cast comment'")]
#[case("COMMENT ON TRANSFORM FOR app.status LANGUAGE plpgsql IS 'transform comment'")]
#[case("COMMENT ON OPERATOR CLASS app.ops USING btree IS 'operator class comment'")]
#[case("COMMENT ON LARGE OBJECT 123 IS 'large object comment'")]
#[case("COMMENT ON AGGREGATE app.a(*) IS 'aggregate comment'")]
#[case("COMMENT ON FUNCTION app.f(IN x integer, INOUT y text) IS 'function comment'")]
#[case("COMMENT ON OPERATOR + (NONE, integer) IS 'operator comment'")]
#[case("ALTER FUNCTION app.f(IN integer, VARIADIC text[]) OWNER TO CURRENT_USER")]
#[case("ALTER AGGREGATE app.a(integer) OWNER TO CURRENT_USER")]
#[case("ALTER AGGREGATE app.a(*) OWNER TO CURRENT_USER")]
#[case("ALTER OPERATOR + (NONE, integer) OWNER TO CURRENT_USER")]
#[case("ALTER OPERATOR CLASS app.ops USING btree OWNER TO CURRENT_USER")]
#[case("ALTER OPERATOR FAMILY app.ops USING btree OWNER TO CURRENT_USER")]
#[case("ALTER COLLATION app.c OWNER TO CURRENT_USER")]
#[case("ALTER CONVERSION app.c OWNER TO CURRENT_USER")]
#[case("ALTER LANGUAGE plpgsql OWNER TO CURRENT_USER")]
#[case("ALTER LARGE OBJECT 123 OWNER TO CURRENT_USER")]
#[case("ALTER STATISTICS app.s OWNER TO CURRENT_USER")]
#[case("ALTER TEXT SEARCH DICTIONARY app.d OWNER TO CURRENT_USER")]
#[case("ALTER TEXT SEARCH CONFIGURATION app.c OWNER TO CURRENT_USER")]
#[case("ALTER EVENT TRIGGER evt OWNER TO CURRENT_USER")]
#[case("ALTER DATABASE appdb OWNER TO CURRENT_USER")]
#[case("ALTER DOMAIN app.d OWNER TO CURRENT_USER")]
#[case("ALTER PROCEDURE app.p(integer) OWNER TO CURRENT_USER")]
#[case("ALTER ROUTINE app.r(integer) OWNER TO CURRENT_USER")]
#[case("ALTER SCHEMA app OWNER TO CURRENT_USER")]
#[case("ALTER TABLESPACE fastspace OWNER TO CURRENT_USER")]
#[case("ALTER TYPE app.status OWNER TO CURRENT_USER")]
#[case("ALTER PUBLICATION pub OWNER TO CURRENT_USER")]
#[case("ALTER SUBSCRIPTION sub OWNER TO CURRENT_USER")]
#[case("ALTER FOREIGN DATA WRAPPER fdw OWNER TO CURRENT_USER")]
#[case("ALTER SERVER srv OWNER TO CURRENT_USER")]
#[case("ALTER SEQUENCE app.s OWNER TO CURRENT_USER")]
#[case("ALTER MATERIALIZED VIEW app.mv OWNER TO CURRENT_USER")]
#[case("ALTER VIEW app.v OWNER TO CURRENT_USER")]
#[case("ALTER TABLE app.t OWNER TO SESSION_USER")]
fn simple_statement_preserves_native_tree(#[case] sql: &str) {
    assert_postgres_structure(sql);
}

#[test]
fn rejects_concurrent_drop_table() {
    let statement = DropTable {
        names: vec![Name::local("t")],
        if_exists: true,
        cascade: false,
        concurrent: true,
    };
    let mut writer = SqlWriter::new(Dialect::postgres());
    let error = statement.emit(&mut writer).expect_err("invalid syntax");
    assert!(error.is_invalid_syntax());
}
