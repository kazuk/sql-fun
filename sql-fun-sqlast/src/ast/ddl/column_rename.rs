//! Resolve once; application consumes the completed binding.
use crate::{
    ast::{AlterCommand, AlterRelation},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{AlterTable, AnalysisError, ColumnRename, SemAst, SourcePathWithSpan},
};
impl AlterRelation {
    pub(super) fn has_column_rename(&self) -> bool {
        self.commands.iter().any(AlterCommand::is_column_rename)
    }
    pub(super) fn resolve_column_rename(
        &self,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("column rename source");
        };
        let [command] = self.commands.as_slice() else {
            return AnalysisError::raise_unexpected_input("column rename must stand alone");
        };
        let (from, to) = command.column_rename_names()?;
        let requested = TableName::from(self.name.full_name()?);
        let (target, rename) =
            ColumnRename::resolve(context, &requested, &from, &to, self.if_exists)?;
        Ok(SemAst::alter_table(&AlterTable::column_rename(
            &requested,
            target.as_ref(),
            &rename,
            source,
        )))
    }
}
impl AlterCommand {
    const fn is_column_rename(&self) -> bool {
        matches!(self, Self::RenameColumn(_, _))
    }
    fn column_rename_names(&self) -> Result<(ColumnName, ColumnName), AnalysisError> {
        let Self::RenameColumn(from, to) = self else {
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
        let statement = relation.resolve_column_rename(&mut context, Some(&source_span()))?;
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
