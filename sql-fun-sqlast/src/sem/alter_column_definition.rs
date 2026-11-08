//! Analyze parser-retained SQLite column replacements without assuming migration semantics.

use crate::{
    ast::{
        AlterCommand as AstAlterCommand, AlterRelation as AstAlterRelation, Column as AstColumn,
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem, ColumnDefinition, SemAst,
        SourcePathWithSpan, problem::AlterColumnProblem,
    },
};

/// A retained column replacement request, distinct from PostgreSQL TYPE/USING.
///
/// The parser's `ALTER COLUMN old TO new definition` extension is not SQLite
/// engine syntax. Its completed bindings and requested definition are retained,
/// together with that rejection; no table-rebuild migration is synthesized.
#[derive(Debug, Clone)]
pub struct AlterColumnDefinition {
    table: TableName,
    column: ColumnName,
    original: Option<ColumnDefinition>,
    requested: ColumnDefinition,
    problems: Vec<AlterColumnProblem>,
}
impl AlterColumnDefinition {
    pub(crate) fn from_ast(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        ast: &AstColumn,
    ) -> Result<Self, AnalysisError> {
        let column = ColumnName::from(name);
        let definition = context.get_table_impl(table.full_name());
        let original = if let Some(table) = definition {
            table.find_column(&column, true).cloned()
        } else {
            None
        };
        let mut problems = vec![AlterColumnProblem::definition_syntax()];
        if definition.is_none() {
            problems.push(AlterColumnProblem::missing_relation());
        } else if original.is_none() {
            problems.push(AlterColumnProblem::missing_column());
        }
        let requested = ColumnDefinition::from_ast_standalone(ast, context, table)?;
        for problem in &problems {
            context
                .report_analysis_problem(AnalysisProblem::alter_column(table, &column, problem))?;
        }
        Ok(Self {
            table: table.clone(),
            column,
            original,
            requested,
            problems,
        })
    }
    /// Canonical table identity, or the requested identity after failed lookup.
    pub const fn table(&self) -> &TableName {
        &self.table
    }
    /// Original column name supplied by the request.
    pub const fn column(&self) -> &ColumnName {
        &self.column
    }
    /// Completed source lookup, including SQLite case-insensitive matching.
    pub const fn original(&self) -> Option<&ColumnDefinition> {
        self.original.as_ref()
    }
    /// Analyzed replacement definition, without applying it to the table.
    pub const fn requested(&self) -> &ColumnDefinition {
        &self.requested
    }
    /// Rejection reasons, independently of diagnostic delivery.
    pub fn problems(&self) -> &[AlterColumnProblem] {
        &self.problems
    }
    /// Whether local analysis accepts the request in the selected dialect.
    pub const fn is_valid(&self) -> bool {
        false
    }

    pub(crate) fn is_ast_relation(ast: &AstAlterRelation) -> bool {
        for command in ast.commands() {
            if matches!(command, AstAlterCommand::AlterColumn(_, _)) {
                return true;
            }
        }
        false
    }
    pub(crate) fn analyze_relation(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ALTER COLUMN source");
        };
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let target = context.resolve_table_name(&requested);
        let table = target.as_ref().unwrap_or(&requested);
        let mut commands = Vec::new();
        for command in ast.commands() {
            if let AstAlterCommand::AlterColumn(name, definition) = command {
                let change = Self::from_ast(context, table, name, definition)?;
                commands.push(AlterObjSubCommand::column_definition(&change));
            } else {
                todo!("SQLite column replacement mixed with other commands");
            }
        }
        Ok(SemAst::alter_table(&AlterTable::analyzed_column_types(
            table,
            target.as_ref(),
            &commands,
            source,
        )))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use crate::{
        ast::Dialect,
        sem::AlterTable,
        test_helpers::{alter_column_type, public_analysis},
    };

    #[test]
    fn parser_extension_retains_bindings_but_is_not_sqlite_engine_syntax() -> testresult::TestResult
    {
        let database = ::rusqlite::Connection::open_in_memory()?;
        database.execute_batch("CREATE TABLE t(a INTEGER)")?;
        let sql = "ALTER TABLE t ALTER COLUMN a TO b TEXT";
        assert!(database.execute_batch(sql).is_err());
        let statement = public_analysis::statement(sql, Dialect::sqlite());
        let result = AlterTable::analyze(&statement, alter_column_type::sqlite_context())?;
        let alter = result.ast().alter_table_statement().unwrap();
        assert!(!alter.can_apply());
        let change = alter.commands()[0].column_definition_change().unwrap();
        assert!(change.original().is_some());
        assert_eq!(change.requested().name().as_ref().unwrap().as_str(), "b");
        assert!(!change.is_valid());
        assert!(
            result
                .context()
                .has_reported_problem("alter_column_definition_syntax")
        );
        Ok(())
    }
}
