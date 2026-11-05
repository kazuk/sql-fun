use crate::{
    ast::{GeneratedColumnMode, IdentityColumnMode},
    names::{ColumnName, FullName, TableName},
    sem::{
        ColumnConstraint, ColumnDefinition, GeneratedColumnConstraint, IdentityColumnConstraint,
        PgBuiltInType, ScalarConstExpr,
    },
    test_helpers::{
        TestParseContext, document_merge::catalog, source_span,
        statements::resolve_postgres as resolve,
    },
};

pub(crate) fn array_catalog() -> TestParseContext {
    let mut context = catalog(17);
    let array = PgBuiltInType::int4().array_type();
    context.setup_type(array.clone());
    context.setup_table(
        "arrays",
        &[
            ColumnDefinition::new(
                &Some(ColumnName::from("id")),
                Some(&PgBuiltInType::int4()),
                Some(true),
                &[],
            ),
            ColumnDefinition::new(&Some(ColumnName::from("a")), Some(&array), Some(false), &[]),
        ],
        &source_span(),
    );
    context
}

pub(crate) fn assert_rejected(sql: &str, version: u32) {
    let mut context = catalog(version);
    let result = resolve(sql, &mut context);
    assert!(
        result.is_err() || context.reported_problem_count() > 0,
        "accepted {sql}"
    );
}

pub(crate) fn assignment_catalog(version: u32) -> TestParseContext {
    let mut context = catalog(version);
    let identity = IdentityColumnConstraint::new(
        &TableName::from(FullName::with_schema("public", "identity_items")),
        &ColumnName::from("id"),
        IdentityColumnMode::always(),
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

    let generated = GeneratedColumnConstraint::new(
        &TableName::from(FullName::with_schema("public", "generated_items")),
        &ColumnName::from("g"),
        ScalarConstExpr::null(),
        Some(GeneratedColumnMode::stored()),
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
    context
}

pub(crate) fn strict_catalog(version: u32) -> TestParseContext {
    let mut context = catalog(version);
    context.setup_table(
        "strict_items",
        &[ColumnDefinition::new(
            &Some(ColumnName::from("id")),
            Some(&PgBuiltInType::int4()),
            Some(true),
            &[],
        )],
        &source_span(),
    );
    context
}
