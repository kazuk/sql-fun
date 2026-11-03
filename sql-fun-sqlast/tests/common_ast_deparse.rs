//! Public CTE insertion and SQL emission checks across both supported dialects.

#[path = "test_helpers/shared.rs"]
mod test_helpers;
use crate::test_helpers::common_ast_deparse::{
    Carrier, assert_cte_attributes, assert_parameter_slots, parse_owned, query_definition,
    source_sql,
};
use ::sql_fun_sqlast::ast::{Dialect, HaveWithClause};

macro_rules! cte_insertion_cases {
    ($name:ident, $dialect:expr) => {
        #[rstest::rstest]
        #[case::select_without_existing_ctes(Carrier::Select, false, 0)]
        #[case::insert_without_existing_ctes(Carrier::Insert, false, 0)]
        #[case::update_without_existing_ctes(Carrier::Update, false, 0)]
        #[case::delete_without_existing_ctes(Carrier::Delete, false, 0)]
        #[case::select_before_existing_ctes(Carrier::Select, true, 0)]
        #[case::select_between_existing_ctes(Carrier::Select, true, 1)]
        #[case::select_after_existing_ctes(Carrier::Select, true, 2)]
        #[case::insert_before_existing_ctes(Carrier::Insert, true, 0)]
        #[case::insert_between_existing_ctes(Carrier::Insert, true, 1)]
        #[case::insert_after_existing_ctes(Carrier::Insert, true, 2)]
        #[case::update_before_existing_ctes(Carrier::Update, true, 0)]
        #[case::update_between_existing_ctes(Carrier::Update, true, 1)]
        #[case::update_after_existing_ctes(Carrier::Update, true, 2)]
        #[case::delete_before_existing_ctes(Carrier::Delete, true, 0)]
        #[case::delete_between_existing_ctes(Carrier::Delete, true, 1)]
        #[case::delete_after_existing_ctes(Carrier::Delete, true, 2)]
        fn $name(#[case] carrier: Carrier, #[case] existing: bool, #[case] position: usize) {
            let dialect = $dialect;
            let mut statement = parse_owned(source_sql(carrier, dialect, existing), dialect);
            let original_sql = statement
                .deparse_sql(dialect)
                .expect("original owned statement emits in its explicit dialect");
            assert_parameter_slots(&original_sql, dialect, usize::from(existing), carrier);

            let definition = query_definition("added", dialect);
            {
                let mut with = statement.ensure_with_clause().expect("DML supports WITH");
                with.insert(position, &definition)
                    .expect("CTE inserts at requested position");
            }

            let emitted = statement
                .deparse_sql(dialect)
                .expect("owned statement emits in its explicit dialect");
            assert_parameter_slots(&emitted, dialect, 1 + usize::from(existing), carrier);

            let reparsed = parse_owned(emitted.clone(), dialect);
            assert_cte_attributes(&reparsed, existing, position);
            assert_eq!(reparsed.deparse_sql(dialect).unwrap(), emitted);

            if dialect == Dialect::postgres() {
                let native = pg_query::parse(&emitted, ::pg_query::ParserOptions::default())
                    .expect("independent PostgreSQL parser accepts emitted SQL")
                    .deparse(::pg_query::DeparseOptions::default())
                    .expect("native PostgreSQL canonical deparse");
                let common = parse_owned(native.clone(), dialect);
                assert_cte_attributes(&common, existing, position);
                let common_sql = common.deparse_sql(dialect).unwrap();
                assert_eq!(
                    pg_query::parse(&common_sql, ::pg_query::ParserOptions::default())
                        .unwrap()
                        .deparse(::pg_query::DeparseOptions::default())
                        .unwrap(),
                    native,
                    "{} carrier at insertion index {position}: {common_sql}",
                    carrier.name()
                );
                assert_parameter_slots(&native, dialect, 1 + usize::from(existing), carrier);
            }

            #[cfg(feature = "sqlite")]
            if dialect == Dialect::sqlite() {
                execute_sqlite_case(&emitted, carrier);
            }
        }
    };
}

cte_insertion_cases!(public_postgres_dml_cte_insertion, Dialect::postgres());

#[cfg(feature = "sqlite")]
cte_insertion_cases!(public_sqlite_dml_cte_insertion, Dialect::sqlite());

#[cfg(feature = "sqlite")]
fn execute_sqlite_case(sql: &str, carrier: Carrier) {
    let connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite database");
    connection
        .execute_batch(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, label TEXT); \
             INSERT INTO items VALUES (4, 'old'), (7, 'delete me');",
        )
        .expect("fixture schema and rows");

    let mut statement = connection
        .prepare(sql)
        .expect("SQLite prepares emitted SQL");
    statement.raw_bind_parameter(7, 7_i64).unwrap();
    statement.raw_bind_parameter(2, 4_i64).unwrap();
    let mut rows = statement.raw_query();
    let row = rows
        .next()
        .unwrap()
        .expect("emitted statement returns a row");
    match carrier {
        Carrier::Select => {
            let total: i64 = row.get(0).unwrap();
            assert_eq!(total, 18);
        }
        Carrier::Insert => {
            let inserted = (
                row.get::<_, i64>(0).unwrap(),
                row.get::<_, String>(1).unwrap(),
            );
            assert_eq!(inserted, (17, "inserted".to_owned()));
        }
        Carrier::Update => {
            let updated = (
                row.get::<_, i64>(0).unwrap(),
                row.get::<_, String>(1).unwrap(),
            );
            assert_eq!(updated, (4, "7".to_owned()));
        }
        Carrier::Delete => {
            let deleted = (
                row.get::<_, i64>(0).unwrap(),
                row.get::<_, String>(1).unwrap(),
            );
            assert_eq!(deleted, (7, "delete me".to_owned()));
            let remaining: i64 = connection
                .query_row("SELECT count(*) FROM items WHERE id = 7", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(remaining, 0);
        }
    }
}
