//! Bindings and admissible parameter assignments against the real operator catalog.
mod test_helpers {
    mod cache_context;
    pub(crate) use self::cache_context::create_cache_context;

    pub(super) mod heavy;
}

use self::test_helpers::heavy;
use ::sql_fun_sqlast::context::ParseContext;
use ::sql_fun_sqlast::sem::PgBuiltInType;
use std::collections::BTreeMap;

// Removed the extra comparison variants from this aggregate; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
#[test]
fn catalog_comparison_bindings_and_constraints() -> testresult::TestResult {
    // Real cross-type operator signatures determine the admissible sources.
    let sql = "SELECT $1 IS DISTINCT FROM 1";
    let analyzed = heavy::analyze(sql)?;
    let constraints = analyzed.context().parameter_constraints();
    let ty = PgBuiltInType::int4();
    assert!(
        constraints.allows(&BTreeMap::from([(1, ty)])),
        "{sql}: {:?}",
        analyzed.context().problems()
    );
    assert!(
        !constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])),
        "{sql}"
    );
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
    Ok(())
}
