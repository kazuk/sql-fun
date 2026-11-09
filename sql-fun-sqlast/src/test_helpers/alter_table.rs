use crate::SqliteDialect;
#[cfg(feature = "sqlite")]
use crate::ast::{AlterRelation, Dialect};
use crate::names::{ColumnName, FullName, SchemaName};
use crate::sem::{ColumnDefinition, TypeReference};
use crate::test_helpers::TestParseContext;
#[cfg(feature = "sqlite")]
use crate::{
    context::{ParseContext, SchemaFileContext},
    names::TableName,
    sem::{AnalysisError, CreateIndex, IndexElementCollection},
    test_helpers::column_rename,
};

#[cfg(feature = "sqlite")]
pub(crate) fn parse_sqlite_alter_table(sql: &str) -> AlterRelation {
    let cmd = SqliteDialect::parse(sql)
        .expect("sqlite parse should succeed")
        .expect("statement should exist");
    let stmt = cmd.into_plain_statement().expect("plain SQLite statement");
    let alter_table = stmt.into_alter_table().expect("ALTER TABLE statement");
    AlterRelation::try_from(&alter_table).expect("common ALTER TABLE syntax")
}

#[cfg(feature = "sqlite")]
pub(crate) fn make_column(name: &str) -> ColumnDefinition {
    ColumnDefinition::new(&Some(ColumnName::from(name)), None, None, &[])
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    context.setup_type(TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "text"),
        false,
    ));
    context
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_drop_index_context() -> Result<SchemaFileContext, AnalysisError> {
    let context = column_rename::sqlite_schema(&["CREATE TABLE t(a,b)"])?;
    // SQLite CREATE INDEX semantic conversion is separately tracked by #552.
    // Supply explicit catalog metadata to exercise DROP's index contract.
    let columns = IndexElementCollection::from_column_names(&vec![ColumnName::from("a")].into());
    let index = CreateIndex::new(
        &FullName::with_schema("main", "ix"),
        &TableName::from(FullName::with_schema("main", "t")),
        &columns,
    );
    context.apply_create_index(&index)
}
