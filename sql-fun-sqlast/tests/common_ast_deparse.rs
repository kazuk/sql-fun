//! Public CTE insertion and SQL emission checks across both supported dialects.

#[path = "test_helpers/shared.rs"]
mod test_helpers;
use crate::test_helpers::common_ast_deparse::{Carrier, exercise_case};
use ::sql_fun_sqlast::ast::Dialect;

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
            exercise_case($dialect, carrier, existing, position);
        }
    };
}

cte_insertion_cases!(public_postgres_dml_cte_insertion, Dialect::postgres());

#[cfg(feature = "sqlite")]
cte_insertion_cases!(public_sqlite_dml_cte_insertion, Dialect::sqlite());
