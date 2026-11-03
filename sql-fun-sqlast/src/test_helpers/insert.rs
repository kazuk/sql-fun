use crate::names::{FullName as NamesFullName, TableName};
use crate::parse_statements;
use crate::sem::{
    CatalogNameLookup as SemCatalogNameLookup, ColumnConstraint, IdentityColumnConstraint,
    IdentityColumnMode,
};
use crate::test_helpers::create_cache_context;
use crate::test_helpers::scalar_catalog::items_catalog;

use crate::ast::Dialect;
use crate::names::ColumnName;
use crate::sem::{ColumnDefinition, PgBuiltInType, SemAst};
use crate::test_helpers::{TestParseContext, source_span};

pub(crate) fn catalog(dialect: Dialect) -> TestParseContext {
    let mut context = items_catalog(dialect, Some(true));

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
    let identity = IdentityColumnConstraint::new(
        &TableName::from(NamesFullName::with_schema("public", "identity_items")),
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
    context
}
pub(crate) fn modifier_analysis(
    target: &str,
    operator: bool,
    requested: &str,
    outcome: &str,
    dialect: Dialect,
) -> (SemAst, TestParseContext, SemCatalogNameLookup) {
    use crate::{
        names::FullName,
        sem::{CatalogNameLookup, InsertArbiter, InsertInferenceModifiers},
    };
    let mut context = catalog(dialect);
    let requested_name = if let Some((schema, local)) = requested.split_once('.') {
        FullName::with_schema(schema, local)
    } else {
        FullName::from_local_name(requested)
    };
    let canonical = if dialect == Dialect::sqlite() {
        FullName::from_local_name("NOCASE")
    } else if requested_name.schema().is_some() {
        requested_name.clone()
    } else {
        FullName::with_schema("pg_catalog", requested)
    };
    let lookup = match outcome {
        "found" | "unmatched" => CatalogNameLookup::found(&requested_name, &canonical),
        "missing" => CatalogNameLookup::missing(&requested_name),
        "ambiguous" => CatalogNameLookup::ambiguous(
            &requested_name,
            &[canonical, FullName::with_schema("custom", "other")],
        ),
        _ => panic!("unknown test outcome"),
    };
    let modifier = if operator {
        context.set_operator_class_lookup(&requested_name, &lookup);
        InsertInferenceModifiers::new(None, Some(lookup.clone()))
    } else {
        context.set_collation_lookup(&requested_name, &lookup);
        InsertInferenceModifiers::new(Some(lookup.clone()), None)
    };
    // The mock independently checks the bound identities passed to inference.
    // Catalog eligibility is deliberately not implemented by the resolver.
    let arbiters = if outcome == "unmatched" {
        Vec::new()
    } else {
        vec![InsertArbiter::index(&FullName::with_schema(
            "public",
            "mock_arbiter",
        ))]
    };
    context.expect_conflict_modifiers(&[modifier], &arbiters);
    let sql = format!("INSERT INTO items VALUES(1,'x') ON CONFLICT({target}) DO NOTHING");
    let statement = parse_statements(&sql, dialect)
        .next()
        .unwrap()
        .unwrap()
        .into_test_node()
        .resolve(&mut context, &mut create_cache_context())
        .unwrap();
    (statement, context, lookup)
}
pub(crate) fn analyze(sql: &str, dialect: Dialect) -> (SemAst, TestParseContext) {
    let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
    let mut context = catalog(dialect);
    let result = statement
        .into_test_node()
        .resolve(&mut context, &mut create_cache_context())
        .expect("semantic analysis");
    (result, context)
}
