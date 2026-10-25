//! Bindings and admissible parameter assignments against the real operator catalog.
mod test_helpers {
    pub(super) mod heavy;
}

use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::{ParseContext, PgBuiltInType};
use std::collections::BTreeMap;

#[test]
fn catalog_comparison_bindings_and_constraints() -> testresult::TestResult {
    // Real cross-type and polymorphic operator signatures are the contract here.
    for sql in [
        "SELECT $1 IS DISTINCT FROM 1",
        "SELECT $1::integer IS DISTINCT FROM 1",
        "SELECT $1 IS DISTINCT FROM '1', $1::integer",
        "SELECT $1 IS DISTINCT FROM ARRAY[1]",
        "SELECT ROW($1,1) IS NOT DISTINCT FROM ROW(1,1)",
    ] {
        let analyzed = heavy::analyze(sql, false)?;
        let constraints = analyzed.context().parameter_constraints();
        let ty = if sql.contains("ARRAY") {
            PgBuiltInType::int4().array_type()
        } else {
            PgBuiltInType::int4()
        };
        assert!(
            constraints.allows(&BTreeMap::from([(1, ty)])),
            "{sql}: {:?}",
            analyzed.context().problems()
        );
        if !sql.contains("::integer") {
            assert!(
                !constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])),
                "{sql}"
            );
        }
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        assert!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .as_distinct()
                .is_some()
        );
    }
    Ok(())
}
