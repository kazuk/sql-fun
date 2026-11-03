#[cfg(feature = "sqlite")]
use ::sql_fun_sqlast::SqliteLexer;

use ::sql_fun_sqlast::ast::{CteDefinition, Dialect, HaveWithClause, Statement};

use ::sql_fun_sqlast::parse_statements;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Carrier {
    Select,
    Insert,
    Update,
    Delete,
}

impl Carrier {
    fn sql(self, dialect: Dialect) -> String {
        let (parameter_7, parameter_2) = if dialect == Dialect::postgres() {
            ("$7", "$2")
        } else {
            ("?7", "?2")
        };
        match self {
            Self::Select => {
                format!("SELECT {parameter_7} + {parameter_2} + {parameter_7} AS total")
            }
            Self::Insert => format!(
                "INSERT INTO items (id, label) VALUES ({parameter_7} + 10, 'inserted') RETURNING id, label"
            ),
            Self::Update => format!(
                "UPDATE items SET label = CAST({parameter_7} AS TEXT) WHERE id = {parameter_2} RETURNING id, label"
            ),
            Self::Delete => {
                format!("DELETE FROM items WHERE id = {parameter_7} RETURNING id, label")
            }
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
        }
    }
}

/// The returned statement owns all syntax after this helper drops its source.
pub(crate) fn parse_owned(source: String, dialect: Dialect) -> Statement {
    let mut statements = parse_statements(&source, dialect);
    let statement = statements
        .next()
        .expect("one statement")
        .expect("statement parses");
    assert!(statements.next().is_none(), "single-statement fixture");
    drop(statements);
    drop(source);
    statement
}

pub(crate) fn query_definition(name: &str, dialect: Dialect) -> CteDefinition {
    let (parameter_7, parameter_2) = if dialect == Dialect::postgres() {
        ("$7", "$2")
    } else {
        ("?7", "?2")
    };
    let body = parse_owned(
        format!("SELECT {parameter_7} AS first_value, {parameter_2} AS second_value"),
        dialect,
    );
    let mut definition = CteDefinition::new(name, &body).expect("query can be a CTE body");
    definition
        .set_columns(&["left_value", "right_value"])
        .expect("valid CTE column names");
    definition.set_materialized(Some(false));
    definition
}

pub(crate) fn source_sql(carrier: Carrier, dialect: Dialect, existing: bool) -> String {
    let carrier_sql = carrier.sql(dialect);
    if existing {
        let (parameter_7, parameter_2) = if dialect == Dialect::postgres() {
            ("$7", "$2")
        } else {
            ("?7", "?2")
        };
        format!(
            "WITH RECURSIVE existing_a(a, b) AS NOT MATERIALIZED (SELECT {parameter_7}, {parameter_2}), existing_b(total) AS MATERIALIZED (SELECT a + b FROM existing_a) {carrier_sql}"
        )
    } else {
        carrier_sql
    }
}

pub(crate) fn expected_names(existing: bool, position: usize) -> Vec<&'static str> {
    if !existing {
        return vec!["added"];
    }
    match position {
        0 => vec!["added", "existing_a", "existing_b"],
        1 => vec!["existing_a", "added", "existing_b"],
        2 => vec!["existing_a", "existing_b", "added"],
        _ => unreachable!("only positions before, between, or after two CTEs are used"),
    }
}

pub(crate) fn assert_cte_attributes(statement: &Statement, existing: bool, position: usize) {
    let with = statement.with_clause().expect("inserted WITH clause");
    assert_eq!(with.is_recursive(), existing);
    assert_eq!(
        with.iter().map(|cte| cte.name()).collect::<Vec<_>>(),
        expected_names(existing, position)
    );

    let added = with.find("added").expect("inserted declaration");
    assert_eq!(
        added.columns().collect::<Vec<_>>(),
        ["left_value", "right_value"]
    );
    assert_eq!(added.materialized(), Some(false));
    if existing {
        let first = with.find("existing_a").expect("first original declaration");
        assert_eq!(first.columns().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(first.materialized(), Some(false));
        assert_eq!(with.find("existing_b").unwrap().materialized(), Some(true));
    }
}

pub(crate) fn parameter_occurrences(sql: &str, dialect: Dialect, parameter: &str) -> usize {
    if dialect == Dialect::postgres() {
        return sql.matches(parameter).count();
    }

    #[cfg(feature = "sqlite")]
    {
        let mut lexer = SqliteLexer::new(sql);
        let mut count = 0;
        while let Some(token) = lexer.next().expect("emitted SQL lexes") {
            if token.token_type() == ::turso_parser::token::TokenType::TK_VARIABLE
                && token.text() == parameter
            {
                count += 1;
            }
        }
        count
    }

    #[cfg(not(feature = "sqlite"))]
    {
        sql.matches(parameter).count()
    }
}

pub(crate) fn assert_parameter_slots(
    sql: &str,
    dialect: Dialect,
    cte_count: usize,
    carrier: Carrier,
) {
    let (parameter_7, parameter_2, parameter_1) = if dialect == Dialect::postgres() {
        ("$7", "$2", "$1")
    } else {
        ("?7", "?2", "?1")
    };
    let (main_7, main_2) = match carrier {
        Carrier::Select => (2, 1),
        Carrier::Insert | Carrier::Delete => (1, 0),
        Carrier::Update => (1, 1),
    };
    assert_eq!(
        parameter_occurrences(sql, dialect, parameter_7),
        main_7 + cte_count,
        "unexpected occurrences of binding slot 7 in {sql}"
    );
    assert_eq!(
        parameter_occurrences(sql, dialect, parameter_2),
        main_2 + cte_count,
        "unexpected occurrences of binding slot 2 in {sql}"
    );
    assert_eq!(
        parameter_occurrences(sql, dialect, parameter_1),
        0,
        "a parameter gap was filled: {sql}"
    );
}

pub(crate) fn exercise_case(dialect: Dialect, carrier: Carrier, existing: bool, position: usize) {
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

#[cfg(feature = "sqlite")]
pub(crate) fn execute_sqlite_case(sql: &str, carrier: Carrier) {
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
