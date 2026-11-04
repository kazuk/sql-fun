use crate::sem::PgBuiltInType;
use crate::sem::convert;
use crate::test_helpers::ast_query::AnalysisFixture;
use crate::test_helpers::create_cache_context;

use crate::ast::{Dialect, Query};
use crate::names::ColumnName;

pub(crate) fn assert_result(syntax: &Query, dialect: Dialect, scenario: &str) {
    let mut catalog = AnalysisFixture::context().with_dialect(dialect);
    let result = convert(syntax)
        .resolve(&mut catalog, &mut create_cache_context())
        .expect("normal query analysis");
    let column = result
        .get_result_column_def(&ColumnName::from("n"))
        .expect("named output column");
    if scenario == "diagnostic" {
        assert_eq!(catalog.reported_problem_count(), 1);
    } else {
        if scenario == "scalar_subqueries" {
            // Scalar-subquery type modeling is outside this stack regression.
            assert!(column.get_type_cloned().is_some());
        } else {
            assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
        }
        assert_eq!(catalog.reported_problem_count(), 0);
    }
}

pub(crate) fn query(scenario: &str) -> String {
    // Deep enough to require growth in debug builds, but below the parser's
    // own limits. Each scenario exercises a distinct semantic recursion edge.
    let depth = if matches!(scenario, "left_joins" | "right_joins" | "set_operations") {
        40
    } else {
        12
    };
    match scenario {
        "left_joins" | "right_joins" => {
            let mut from = "(select 42 as n) a0".to_owned();
            for index in 1..depth {
                let next = format!("(select 42 as n) a{index}");
                from = if scenario == "left_joins" {
                    format!("({from} cross join {next})")
                } else {
                    format!("({next} cross join {from})")
                };
            }
            format!("select a0.n as n from {from}")
        }
        "expressions" | "diagnostic" => {
            let mut expression = if scenario == "diagnostic" {
                "missing".to_owned()
            } else {
                "42".to_owned()
            };
            for _ in 0..depth {
                expression = if scenario == "diagnostic" {
                    format!("({expression})")
                } else {
                    format!("case when true then {expression} else 42 end")
                };
            }
            format!("select {expression} as n")
        }
        "scalar_subqueries" => {
            let mut sql = "select 42 as n".to_owned();
            for _ in 0..depth {
                sql = format!("select ({sql}) as n");
            }
            sql
        }
        "subqueries" => {
            let mut sql = "select 42 as n".to_owned();
            for _ in 0..depth {
                sql = format!("select s.n as n from ({sql}) s");
            }
            sql
        }
        "ctes" => {
            let mut sql = "select 42 as n".to_owned();
            for _ in 0..depth {
                sql = format!("with c as ({sql}) select n from c");
            }
            sql
        }
        "set_operations" => {
            let mut sql = "select 42 as n".to_owned();
            for _ in 0..depth {
                sql.push_str(" union select 42 as n");
            }
            sql
        }
        _ => panic!("unknown regression scenario"),
    }
}
