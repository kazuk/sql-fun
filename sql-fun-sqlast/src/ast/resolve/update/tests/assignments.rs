use crate::ast::Dialect;

#[test]
fn synthetic_array_parameters_retain_assignment_compatibility() {
    let mut context = test_helpers::catalog();
    let result = crate::parse_statements("UPDATE arrays SET a[$1]=$2", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(
            &mut context,
            &mut crate::test_helpers::create_cache_context(),
        );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(context.reported_problem_count(), 0);
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::sem::{ColumnDefinition, ColumnName, PgBuiltInType};
    use crate::test_helpers::{TestParseContext, source_span};

    pub(super) fn catalog() -> TestParseContext {
        let mut context = super::super::test_helpers::catalog(Dialect::postgres());
        let array =
            crate::sem::TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
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
