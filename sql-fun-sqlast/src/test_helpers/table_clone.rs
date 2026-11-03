use crate::test_helpers::source_span;

use crate::names::{ColumnName, FullName, TableName};
use crate::sem::{
    AlterObjSubCommand, AlterTable, AnalysisError, ColumnDefinition, Constraint, CreateTable,
    CreateTableMode, PgBuiltInType, PrimaryKeyConstraint,
};

pub(crate) fn table_with_primary_key() -> Result<CreateTable, AnalysisError> {
    let name = TableName::from(FullName::with_schema("public", "items"));
    let id = ColumnName::from("id");
    let source = source_span();
    let mut table = CreateTable::new(
        &name,
        vec![ColumnDefinition::new(
            &Some(id.clone()),
            Some(&PgBuiltInType::int4()),
            Some(true),
            &[],
        )],
        CreateTableMode::from_if_not_exists(false),
        false,
        &source,
    );
    let primary_key = PrimaryKeyConstraint::new("items_pk", &name, &[id]);
    let constraint = Constraint::primary_key(&primary_key);
    let alteration = AlterTable::new(
        &name,
        &[AlterObjSubCommand::add_constraint(&constraint)],
        &source,
    );
    table.apply_alter(&alteration)?;
    Ok(table)
}
