#[cfg(feature = "sqlite")]
use ::sql_fun_sqlast::SqliteLexer;

use ::sql_fun_sqlast::ast::{CteDefinition, Dialect, Statement};

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

    pub(crate) const fn name(self) -> &'static str {
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
