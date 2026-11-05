//! Semantic regressions through both parser frontends on explicit small stacks.
//! Parsing/conversion gets 2 MiB; resolution also exercises 512 KiB host threads.

#[cfg(feature = "sqlite")]
use crate::test_helpers::ast_query::sqlite;

use crate::{
    ast::Dialect,
    test_helpers::{
        ast_query::postgres, on_small_stack, small_stack_regression,
        stack_regressions as test_helpers,
    },
};

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
        let syntax = on_small_stack(|| sqlite(&sql).expect("legal SQLite query"));
        test_helpers::assert_result(&syntax, Dialect::sqlite(), scenario);
    });
}
