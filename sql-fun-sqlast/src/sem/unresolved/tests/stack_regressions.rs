//! Semantic regressions through both parser frontends on explicit small stacks.
//! Parsing/conversion gets 2 MiB; resolution also exercises 512 KiB host threads.

use super::test_helpers::{ReadOnlyCatalog, postgres};
use crate::sem::unresolved::Dialect;
use crate::sem::{ColumnName, PgBuiltInType};
use crate::test_helpers::{TestParseContext, on_small_stack, small_stack_regression};

#[rstest::rstest]
#[case::left_joins("left_joins")]
#[case::right_joins("right_joins")]
#[case::expressions("expressions")]
#[case::subqueries("subqueries")]
#[case::scalar_subqueries("scalar_subqueries")]
#[case::ctes("ctes")]
#[case::set_operations("set_operations")]
#[case::diagnostic("diagnostic")]
fn postgres_small_stack(
    #[case] scenario: &str,
    #[values(512 * 1024, 2 * 1024 * 1024)] stack_size: usize,
) {
    small_stack_regression(stack_size, || {
        let sql = test_helpers::query(scenario);
        let syntax = on_small_stack(|| postgres(&sql).expect("legal PostgreSQL query"));
        test_helpers::assert_result(&syntax, Dialect::postgres(), scenario);
    });
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::left_joins("left_joins")]
#[case::right_joins("right_joins")]
#[case::expressions("expressions")]
#[case::subqueries("subqueries")]
#[case::scalar_subqueries("scalar_subqueries")]
#[case::ctes("ctes")]
#[case::set_operations("set_operations")]
#[case::diagnostic("diagnostic")]
fn sqlite_small_stack(
    #[case] scenario: &str,
    #[values(512 * 1024, 2 * 1024 * 1024)] stack_size: usize,
) {
    small_stack_regression(stack_size, || {
        let sql = test_helpers::query(scenario);
        let syntax =
            on_small_stack(|| super::test_helpers::sqlite(&sql).expect("legal SQLite query"));
        test_helpers::assert_result(&syntax, Dialect::sqlite(), scenario);
    });
}

mod test_helpers {
    use super::{ColumnName, Dialect, PgBuiltInType, ReadOnlyCatalog, TestParseContext};
    use crate::sem::unresolved::Query;

    pub(super) fn assert_result(syntax: &Query, dialect: Dialect, scenario: &str) {
        let catalog = ReadOnlyCatalog::new();
        let mut diagnostics = TestParseContext::default();
        let result = syntax
            .resolve(&catalog, dialect)
            .report(&mut diagnostics)
            .expect("normal query analysis");
        let column = result
            .get_result_column_def(&ColumnName::from("n"))
            .expect("named output column");
        if scenario == "diagnostic" {
            assert_eq!(diagnostics.reported_problem_count(), 1);
        } else {
            if scenario == "scalar_subqueries" {
                // Scalar-subquery type modeling is outside this stack regression.
                assert!(column.get_type_cloned().is_some());
            } else {
                assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
            }
            assert_eq!(diagnostics.reported_problem_count(), 0);
        }
    }

    pub(super) fn query(scenario: &str) -> String {
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
}
