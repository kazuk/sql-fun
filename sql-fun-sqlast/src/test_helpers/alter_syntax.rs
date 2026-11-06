//! Parser-built ALTER fixtures converted to common syntax before analysis.
use crate::{
    ast::{AlterCommand, AlterRelation, TryFromWithTokens},
    names::FullName,
    sem::AnalysisError,
    syn::{AliasOpt, ListOpt, NodeInner, ObjectType, Opt},
    test_helpers::SynBuilder,
};
use ::sql_fun_core::IVec;

pub(crate) fn owner() -> Result<AlterCommand, AnalysisError> {
    let command = SynBuilder::new().alter_table_cmd_change_owner("test");
    AlterCommand::try_from_with_tokens(&command, &IVec::default())
}

pub(crate) fn default_command() -> Result<AlterCommand, AnalysisError> {
    let builder = SynBuilder::new();
    let command = builder.alter_table_cmd_set_column_default(
        "value",
        NodeInner::AConst(builder.const_int4(0)).into(),
    );
    AlterCommand::try_from_with_tokens(&command, &IVec::default())
}

pub(crate) fn default_statement() -> Result<AlterRelation, AnalysisError> {
    let builder = SynBuilder::new();
    let relation = builder.range_var(
        &FullName::with_schema("public", "table1"),
        &AliasOpt::none(),
    );
    let command = builder.alter_table_cmd_set_column_default(
        "col1",
        NodeInner::AConst(builder.const_int4(10)).into(),
    );
    let commands = builder.as_node_list(&[NodeInner::AlterTableCmd(command).into()]);
    let statement = builder.alter_table_stmt(relation.into(), commands, ObjectType::ObjectTable);
    AlterRelation::try_from_with_tokens(&statement, &IVec::default())
}

pub(crate) fn command(sql: &str) -> Result<AlterCommand, AnalysisError> {
    let node = SynBuilder::new().parse(sql);
    let statement = node.as_alter_table_stmt().as_inner().expect("ALTER TABLE");
    let commands = statement.get_cmds().as_inner().expect("commands");
    let command = commands[0]
        .as_alter_table_cmd()
        .as_inner()
        .expect("command");
    AlterCommand::try_from_with_tokens(&command, &IVec::default())
}
