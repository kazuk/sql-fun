use crate::{
    ast::CreateTableMode,
    context::{FrozenBaseContext, ParseContext as _, SchemaFileContext, SourceAccessService},
    names::{ColumnName, FullName, SchemaName, TableName},
    sem::{
        AlterObjSubCommand, AlterTable, ColumnDefinition, Constraint, CreateTable, PgBuiltInType,
        PrimaryKeyConstraint,
    },
    test_helpers::{TestParseContext, source_span},
};
use std::rc::{Rc, Weak};

pub(crate) fn empty_base() -> FrozenBaseContext {
    let mut context = TestParseContext::default();
    context.set_search_path(&[SchemaName::from("public")]);
    context.freeze()
}

fn column(name: &str) -> ColumnDefinition {
    ColumnDefinition::new(
        &Some(ColumnName::from(name)),
        Some(&PgBuiltInType::int4()),
        Some(true),
        &[],
    )
}

pub(crate) fn table_with_id() -> CreateTable {
    CreateTable::new(
        &TableName::from(FullName::with_schema("public", "items")),
        vec![column("id")],
        CreateTableMode::from_if_not_exists(false),
        false,
        &source_span(),
    )
}

pub(crate) fn table_with_key_candidates() -> CreateTable {
    CreateTable::new(
        &TableName::from(FullName::with_schema("public", "items")),
        vec![column("id"), column("other_id")],
        CreateTableMode::from_if_not_exists(false),
        false,
        &source_span(),
    )
}

pub(crate) fn add_primary_key(table: &CreateTable, name: &str) -> AlterTable {
    let primary_key =
        PrimaryKeyConstraint::new("items_pk", table.name(), &[ColumnName::from(name)]);
    let constraint = Constraint::primary_key(&primary_key);
    AlterTable::new(
        table.name(),
        &[AlterObjSubCommand::add_constraint(&constraint)],
        &source_span(),
    )
}

#[derive(Debug)]
struct LocalSource {
    _lifetime: Rc<()>,
}

impl SourceAccessService for LocalSource {}

pub(crate) fn attach_local_source(context: &mut SchemaFileContext) -> Weak<()> {
    let lifetime = Rc::new(());
    let observer = Rc::downgrade(&lifetime);
    context.set_source_file(Box::new(LocalSource {
        _lifetime: lifetime,
    }));
    observer
}
