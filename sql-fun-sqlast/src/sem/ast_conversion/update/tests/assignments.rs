use crate::sem::convert;
use crate::{ast::Dialect, parse_statements, test_helpers::create_cache_context};

#[test]
fn synthetic_array_parameters_retain_assignment_compatibility() {
    let mut context = test_helpers::catalog();
    let result = convert(
        &parse_statements("UPDATE arrays SET a[$1]=$2", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .node,
    )
    .resolve(&mut context, &mut create_cache_context());
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(context.reported_problem_count(), 0);
}

use crate::test_helpers::update_assignments as test_helpers;
