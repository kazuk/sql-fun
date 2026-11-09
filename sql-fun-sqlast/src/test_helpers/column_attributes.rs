pub(crate) use crate::test_helpers::ownership::apply_ownership_sql as apply;
use crate::{
    ast::{CreateTableMode, GeneratedColumnMode},
    context::{
        BaseParseContext as _, FrozenBaseContext, MutSourcePathWithSpan as _, ParseContext as _,
        SchemaFileContext, TableInfoWrite,
    },
    names::{AliasName, ColumnName, FullName, SchemaName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnDefinition, ColumnReferenceExpr, CreateTable,
        GeneratedColumnConstraint, PgBuiltInType as SemPgBuiltInType, SemScalarExpr,
    },
    test_helpers::{TestParseContext, source_span},
};

// pg_query 6 uses PostgreSQL 17 grammar. Seed the supported semantic catalog
// mode directly so PostgreSQL 18 virtual-column restrictions are exercised.

pub(crate) fn virtual_column_context() -> SchemaFileContext {
    use crate::{
        ast::{CreateTableMode, GeneratedColumnMode},
        context::TableInfoWrite,
        sem::{
            ColumnConstraint, CreateTable, GeneratedColumnConstraint, PgBuiltInType,
            ScalarConstExpr, SemScalarExpr,
        },
    };
    let mut context = context("18");
    let column = ColumnName::from("g");
    let constraint = ColumnConstraint::generated(GeneratedColumnConstraint::new(
        &table_name(),
        &column,
        SemScalarExpr::new_const(ScalarConstExpr::new_integer(1)),
        Some(GeneratedColumnMode::r#virtual()),
    ));
    let table = CreateTable::new(
        &table_name(),
        vec![ColumnDefinition::new(
            &Some(column),
            Some(&PgBuiltInType::int4()),
            Some(false),
            &[constraint],
        )],
        CreateTableMode::from_if_not_exists(false),
        false,
        &source_span(),
    );
    context.insert_create_table(table_name().full_name(), &table);
    context
}

pub(crate) fn table_name() -> TableName {
    TableName::from(FullName::with_schema("public", "t"))
}
pub(crate) fn context(version: &str) -> SchemaFileContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({"engine":"postgres", "dialect":"postgres", "version":version, "search_path":["public"]})).unwrap();
    let mut inventory = TestParseContext::default().with_metadata(&metadata);
    inventory.set_search_path(&[SchemaName::from("public")]);
    inventory.setup_type(SemPgBuiltInType::int4());
    child(inventory.freeze())
}
pub(crate) fn child(parent: FrozenBaseContext) -> SchemaFileContext {
    let mut context = SchemaFileContext::new(
        std::path::Path::new("attributes.sql"),
        &parent,
        parent.get_args(),
        parent.metadata(),
    );
    context.set_source_path_with_span(&source_span());
    context
}

pub(crate) fn seed(sql: &str, version: &str) -> Result<SchemaFileContext, AnalysisError> {
    Ok(apply(context(version), sql)?.into_context())
}
pub(crate) fn column<'a>(context: &'a SchemaFileContext, name: &str) -> &'a ColumnDefinition {
    context
        .get_table(&table_name())
        .unwrap()
        .get_column_def(&ColumnName::from(name))
        .unwrap()
}
pub(crate) fn assert_select_nullability(
    context: SchemaFileContext,
    column: &str,
    expected: Option<bool>,
) -> Result<(), AnalysisError> {
    let result = apply(context, &format!("SELECT {column} FROM t"))?;
    let query = result.ast().select_statement().unwrap();
    assert_eq!(
        query
            .get_result_column_def(&ColumnName::from(column))
            .unwrap()
            .is_not_null(),
        expected
    );
    Ok(())
}

/// Explicit completed/failed bindings avoid relying on the current generation parser gap.
pub(crate) fn generation_dependency_context(
    known: bool,
) -> Result<SchemaFileContext, AnalysisError> {
    let mut context = context("17");
    let name = table_name();
    let a = ColumnName::from("a");
    let g = ColumnName::from("g");
    let original =
        ColumnDefinition::new(&Some(a.clone()), Some(&SemPgBuiltInType::int4()), None, &[]);
    let expression = if known {
        SemScalarExpr::column_reference(ColumnReferenceExpr::table_column(
            &AliasName::from("t"),
            &a,
            &name,
            Some(&original),
            false,
        ))
    } else {
        ColumnReferenceExpr::undefined_column_with_report(&mut context, &a)?
    };
    let generated = ColumnConstraint::generated(GeneratedColumnConstraint::new(
        &name,
        &g,
        expression,
        Some(GeneratedColumnMode::stored()),
    ));
    let table = CreateTable::new(
        &name,
        vec![
            original,
            ColumnDefinition::new(
                &Some(g),
                Some(&SemPgBuiltInType::int4()),
                None,
                &[generated],
            ),
            ColumnDefinition::new(
                &Some(ColumnName::from("b")),
                Some(&SemPgBuiltInType::int4()),
                None,
                &[],
            ),
        ],
        CreateTableMode::from_if_not_exists(false),
        false,
        &source_span(),
    );
    context.insert_create_table(name.full_name(), &table);
    Ok(context)
}
