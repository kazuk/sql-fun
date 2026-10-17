use crate::ast::Dialect;

#[rstest::rstest]
#[case::identity_value("UPDATE identity_items SET id=2", false)]
#[case::identity_default("UPDATE identity_items SET id=DEFAULT", true)]
#[case::generated_value("UPDATE generated_items SET g=2", false)]
#[case::generated_default("UPDATE generated_items SET g=DEFAULT", true)]
#[case::element_default("UPDATE arrays SET a[1]=DEFAULT", false)]
#[case::array_element("UPDATE arrays SET a[1]=2", true)]
#[case::array_parameter("UPDATE arrays SET a[$1]=$2", true)]
#[case::multiple_elements("UPDATE arrays SET a[1]=2,a[2]=3", true)]
#[case::overlap("UPDATE arrays SET a=DEFAULT,a[1]=2", false)]
fn validates_destination_rules(#[case] sql: &str, #[case] valid: bool) {
    let mut context = test_helpers::catalog();
    let result = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        context.reported_problem_count() == 0,
        valid,
        "{:?}",
        context.reported_problems()
    );
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::sem::{
        ColumnConstraint, ColumnDefinition, ColumnName, FullName, PgBuiltInType, TableName,
    };
    use crate::test_helpers::{TestParseContext, source_span};

    pub(super) fn catalog() -> TestParseContext {
        let mut context = super::super::test_helpers::catalog(Dialect::postgres());
        let identity = crate::sem::IdentityColumnConstraint::new(
            &TableName::from(FullName::with_schema("public", "identity_items")),
            &ColumnName::from("id"),
            crate::sem::IdentityColumnMode::always(),
        );
        context.setup_table(
            "identity_items",
            &[ColumnDefinition::new(
                &Some(ColumnName::from("id")),
                Some(&PgBuiltInType::int4()),
                Some(true),
                &[ColumnConstraint::identity(identity)],
            )],
            &source_span(),
        );
        let generated = crate::sem::GeneratedColumnConstraint::new(
            &TableName::from(FullName::with_schema("public", "generated_items")),
            &ColumnName::from("g"),
            crate::sem::ScalarConstExpr::null(),
            Some(crate::sem::GeneratedColumnMode::stored()),
        );
        context.setup_table(
            "generated_items",
            &[ColumnDefinition::new(
                &Some(ColumnName::from("g")),
                Some(&PgBuiltInType::int4()),
                Some(false),
                &[ColumnConstraint::generated(generated)],
            )],
            &source_span(),
        );
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
