use crate::ast::AlterRelation;
use crate::names::FullName;
use crate::sem::RelationAlteration;
use crate::{
    ast::AlterCommand,
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{AlterTable, AnalysisError, ColumnRename, SemAst, SourcePathWithSpan},
};

impl RelationAlteration {
    pub(crate) fn has_ast_column_rename(ast: &AlterRelation) -> bool {
        ast.commands.iter().any(ColumnRename::is_ast_command)
    }

    pub(crate) fn from_ast_column_rename(
        ast: &AlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("column rename source");
        };
        let [command] = ast.commands.as_slice() else {
            return AnalysisError::raise_unexpected_input("column rename must stand alone");
        };
        let (from, to) = ColumnRename::names_from_ast(command)?;
        let requested = TableName::from(FullName::from_ast_name(&ast.name)?);
        let (target, rename) =
            ColumnRename::resolve(context, &requested, &from, &to, ast.if_exists)?;
        Ok(SemAst::alter_table(&AlterTable::column_rename(
            &requested,
            target.as_ref(),
            &rename,
            source,
        )))
    }
}

impl ColumnRename {
    pub(crate) const fn is_ast_command(ast: &AlterCommand) -> bool {
        matches!(ast, AlterCommand::RenameColumn(_, _))
    }

    pub(crate) fn names_from_ast(
        ast: &AlterCommand,
    ) -> Result<(ColumnName, ColumnName), AnalysisError> {
        let AlterCommand::RenameColumn(from, to) = ast else {
            return AnalysisError::raise_unexpected_input("column rename command");
        };
        Ok((
            ColumnName::from(from.as_str()),
            ColumnName::from(to.as_str()),
        ))
    }
}

#[cfg(test)]
mod applicability_tests {
    use crate::sem::RelationAlteration;

    use crate::ast::StatementNode;

    use crate::{
        ParseContext,
        context::TableInfoRead,
        test_helpers::{
            column_rename::{name, schema},
            ownership::ownership_statement,
            source_span,
        },
    };
    #[test]
    fn suppressed_problems_do_not_authorize_application() -> testresult::TestResult {
        let mut context = schema(&["CREATE TABLE t(a integer, b integer)"])?;
        let syntax = ownership_statement("ALTER TABLE t RENAME a TO b")?;
        let StatementNode::AlterRelation(relation) = syntax else {
            panic!("ALTER TABLE");
        };
        context.begin_parameter_type_analysis();
        let statement = RelationAlteration::from_ast_column_rename(
            &relation,
            &mut context,
            Some(&source_span()),
        )?;
        assert_eq!(context.analysis_state_mut().take_problems().len(), 1);
        assert!(!statement.alter_table_statement().unwrap().can_apply());
        let context = statement.apply_relation_alter(context)?;
        assert_eq!(
            context
                .get_table_impl(&name("t"))
                .unwrap()
                .alter_statements()
                .len(),
            0
        );
        Ok(())
    }
}
