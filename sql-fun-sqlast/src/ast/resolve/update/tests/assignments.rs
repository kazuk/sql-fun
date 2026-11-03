use crate::{ast::Dialect, parse_statements, test_helpers::create_cache_context};

#[test]
fn synthetic_array_parameters_retain_assignment_compatibility() {
    let mut context = test_helpers::catalog();
    let result = parse_statements("UPDATE arrays SET a[$1]=$2", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context, &mut create_cache_context());
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(context.reported_problem_count(), 0);
}

mod test_helpers {
    use crate::ast::resolve::update::tests::test_helpers::catalog as test_helpers_catalog;

    use crate::{
        ast::Dialect,
        names::ColumnName,
        sem::{ColumnDefinition, PgBuiltInType},
        test_helpers::{TestParseContext, source_span},
    };

    pub(super) fn catalog() -> TestParseContext {
        let mut context = test_helpers_catalog(Dialect::postgres());
        let array = PgBuiltInType::int4().array_type();
        context.setup_type(array.clone());
        context.setup_table(
            "arrays",
            &[ColumnDefinition::new(
                &Some(ColumnName::from("a")),
                Some(&array),
                Some(false),
                &[],
            )],
            &source_span(),
        );
        context
    }
}
