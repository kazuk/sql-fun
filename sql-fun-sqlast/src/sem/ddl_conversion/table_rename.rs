use crate::ast::AlterRelation;
use crate::sem::RelationAlteration;

use crate::{
    ast::{AlterCommand, Dialect},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{AlterTable, AnalysisError, AnalysisProblem, Kind, SemAst, SourcePathWithSpan},
};

impl RelationAlteration {
    pub(crate) fn has_ast_table_rename(ast: &AlterRelation) -> bool {
        ast.commands.iter().any(AlterTable::is_ast_table_rename)
    }

    pub(crate) fn from_ast_table_rename(
        ast: &AlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("table rename source");
        };
        let [command] = ast.commands.as_slice() else {
            return AnalysisError::raise_unexpected_input("table rename must stand alone");
        };
        let requested = TableName::from(FullName::from_ast_name(&ast.name)?);
        let target = context.resolve_table_name(&requested).map(|target| {
            if context.dialect() == Dialect::sqlite() {
                TableName::from(target.full_name().sqlite_identity())
            } else {
                target
            }
        });
        if target.is_none() {
            let existing = context.resolve_relation_name(requested.full_name());
            if !ast.if_exists || existing.is_some() {
                context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            }
        }
        let original = target.as_ref().unwrap_or(&requested).full_name();
        let destination = AlterTable::rename_destination_from_ast(command, original)?;
        let destination = if context.dialect() == Dialect::sqlite() {
            TableName::from(destination.full_name().sqlite_identity())
        } else {
            destination
        };
        let permitted = if target.is_some() {
            context.permits_relation_creation(
                destination.full_name(),
                Kind::table(),
                false,
                false,
            )?
        } else {
            false
        };
        Ok(SemAst::alter_table(&AlterTable::table_rename(
            &requested,
            target.as_ref(),
            &destination,
            permitted,
            source,
        )))
    }
}

impl AlterTable {
    pub(crate) const fn is_ast_table_rename(ast: &AlterCommand) -> bool {
        matches!(ast, AlterCommand::Rename(_))
    }

    pub(crate) fn rename_destination_from_ast(
        ast: &AlterCommand,
        original: &FullName,
    ) -> Result<TableName, AnalysisError> {
        let AlterCommand::Rename(name) = ast else {
            return AnalysisError::raise_unexpected_input("table rename command");
        };
        let local = FullName::from_ast_name(name)?;
        Ok(TableName::from(FullName::new(
            &original.catalog().cloned(),
            &original.schema().cloned(),
            local.name(),
        )))
    }
}

#[cfg(test)]
mod test_applicability {
    use crate::sem::RelationAlteration;

    use crate::{
        ast::StatementNode,
        test_helpers::{schema_layer, source_span},
    };

    use crate::{
        ParseContext,
        context::TableInfoRead,
        names::FullName,
        test_helpers::ownership::{apply_ownership_sql, ownership_context, ownership_statement},
    };

    #[test]
    fn suppressed_diagnostics_cannot_authorize_a_conflicting_rename() -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE before_name(id integer)")?
                .into_context();
        let mut context =
            apply_ownership_sql(context, "CREATE TABLE occupied(id integer)")?.into_context();
        let syntax = ownership_statement("ALTER TABLE before_name RENAME TO occupied")?;
        let StatementNode::AlterRelation(relation) = syntax else {
            panic!("ALTER TABLE syntax");
        };
        context.begin_parameter_type_analysis();
        let ast = RelationAlteration::from_ast_table_rename(
            &relation,
            &mut context,
            Some(&source_span()),
        )?;
        assert_eq!(context.problems().len(), 1);
        // Diagnostic delivery is independent of the retained applicability decision.
        let context = schema_layer(std::path::Path::new("rename.sql"), &context.freeze());
        assert!(context.problems().is_empty());
        assert!(!ast.alter_table_statement().unwrap().can_apply());
        let context = ast.apply_relation_alter(context)?;
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "before_name"))
                .is_some()
        );
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "occupied"))
                .is_some()
        );
        Ok(())
    }
}
