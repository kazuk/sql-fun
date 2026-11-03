use crate::context::{
    BaseParseContext as _, FrozenBaseContext, MutSourcePathWithSpan as _, ParseContext as _,
    SchemaFileContext,
};
use crate::names::{ColumnName, FullName, SchemaName, TableName};
use crate::sem::{AnalysisError, ColumnDefinition, PgBuiltInType as SemPgBuiltInType};
use crate::test_helpers::TestParseContext;
pub(crate) use crate::test_helpers::ownership::apply_ownership_sql as apply;
use crate::test_helpers::source_span;

// pg_query 6 uses PostgreSQL 17 grammar. Seed the supported semantic catalog
// mode directly so PostgreSQL 18 virtual-column restrictions are exercised.

pub(crate) fn virtual_column_context() -> SchemaFileContext {
    use crate::{
        context::TableInfoWrite,
        sem::{
            ColumnConstraint, CreateTable, CreateTableMode, GeneratedColumnConstraint,
            GeneratedColumnMode, PgBuiltInType, ScalarConstExpr, SemScalarExpr,
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
