//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        ColumnConstraint as AstColumnConstraint, ForeignReference as AstForeignReference,
        TableConstraint as AstTableConstraint,
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterObjSubCommand, AnalysisError, AnalysisProblem, CheckConstraint, Constraint,
        CreateTable, ForeignKeyConstraint, ForeignKeyTarget, PrimaryKeyConstraint,
        UniqueConstraint,
    },
};

impl Constraint {
    pub(crate) fn from_ast_declared_key(
        ast: &AstTableConstraint,
        context: &mut impl ParseContext,
        table: &CreateTable,
    ) -> Result<Option<Self>, AnalysisError> {
        if !ast.constraint().is_unique_key() {
            return Ok(None);
        }
        if ast.extras().deferrable() {
            todo!("deferrable unique-key catalog representation");
        }
        let keys = Self::columns_from_ast(ast);
        table.check_reference_columns(context, table.name(), &keys)?;
        Self::from_ast_column_table(
            ast.constraint(),
            context,
            table.name(),
            ast.name().as_deref().unwrap_or_default(),
            &keys,
        )
        .map(Some)
    }

    pub(crate) fn from_ast_table(
        ast: &AstTableConstraint,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        if ast.is_check() {
            let definition = context.get_table(table).cloned();
            return CheckConstraint::from_ast_table_constraint(
                ast,
                context,
                table,
                definition.as_ref(),
                true,
            )
            .map(Constraint::Check);
        }
        let keys = Self::columns_from_ast(ast);
        if context.get_table(table).is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(table))?;
        }
        for key in &keys {
            AlterObjSubCommand::check_ast_missing_column(context, table, key)?;
        }
        if ast.constraint().is_unique_key() && ast.extras().deferrable() {
            todo!("deferrable unique-key catalog representation");
        }
        let name = ast.name().as_deref().unwrap_or_default();
        let mut constraint =
            Self::from_ast_column_table(ast.constraint(), context, table, name, &keys)?;
        constraint.set_foreign_key_delete_columns(&Self::delete_columns_from_ast(ast));
        Ok(constraint)
    }
}

#[cfg(test)]
mod test_analyze_alter_obj_add_constraint {
    use crate::{
        ast::CreateTableMode,
        names::{FullName, TableName},
        sem::{Constraint, CreateTable, SourcePathWithSpan},
        test_helpers::{TestParseContext, foreign_keys, source_span, test_context},
    };
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_obj_add_constraint(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        let constraint = foreign_keys::synthetic_primary_constraint();
        let table_name = TableName::from(FullName::with_schema("public", "table1"));
        let create_table = CreateTable::new(
            &table_name,
            vec![],
            CreateTableMode::Default,
            false,
            &source_span,
        );
        test_context.set_get_table_impl_result(table_name.full_name(), &Some(create_table));
        let _result = Constraint::from_ast_table(&constraint, &mut test_context, &table_name)?;
        Ok(())
    }
}

#[cfg(test)]
mod test_analyze {
    use crate::{sem::Constraint, test_helpers::foreign_keys as fk};

    #[rstest::rstest]
    #[case::explicit("parent(id)")]
    fn direct_analysis_retains_the_matched_catalog_key(
        #[case] reference: &str,
    ) -> testresult::TestResult {
        let mut context = fk::apply(fk::parents(), "CREATE TABLE a.child(id integer)");
        let syntax = fk::postgres_table_constraint(&format!(
            "ALTER TABLE a.child ADD FOREIGN KEY(id) REFERENCES {reference}"
        ));
        let result = Constraint::from_ast_table(&syntax, &mut context, &fk::name("a", "child"))?;
        let result = result.as_foreign_key().unwrap();
        assert_eq!(result.target_table(), Some(&fk::name("a", "parent")));
        assert_eq!(
            result.target_columns(),
            Some(&[crate::names::ColumnName::from("id")][..])
        );
        fk::reversed_path(&mut context);
        assert_eq!(result.target_table(), Some(&fk::name("a", "parent")));
        assert!(context.problems().is_empty());
        Ok(())
    }
    #[test]
    fn common_ast_expands_omitted_columns_from_the_bound_parent() {
        use crate::context::TableInfoRead;
        let context = fk::bind(fk::parents(), "child", "parent", true);
        let table = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        let constraint = table.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.target_columns(),
            Some(&[crate::names::ColumnName::from("id")][..])
        );
        assert_eq!(constraint.target_table(), Some(&fk::name("a", "parent")));
        assert!(context.problems().is_empty());
    }
}

impl Constraint {
    pub(crate) fn from_ast_column_table(
        ast: &AstColumnConstraint,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Self, AnalysisError> {
        match ast {
            AstColumnConstraint::PrimaryKey(_, _) => {
                Ok(Self::ast_table_primary_key(table, name, keys))
            }
            AstColumnConstraint::Unique(_) => Ok(Self::ast_table_unique(table, name, keys)),
            AstColumnConstraint::ForeignKey(reference) => {
                Self::from_ast_foreign_reference(reference, context, table, name, keys)
            }
            _ => todo!(),
        }
    }

    pub(crate) fn ast_table_primary_key(
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Self {
        Self::primary_key(&PrimaryKeyConstraint::new(name, table, keys))
    }

    pub(crate) fn ast_table_unique(table: &TableName, name: &str, keys: &[ColumnName]) -> Self {
        Self::unique(&UniqueConstraint::new(name, table, keys))
    }

    pub(crate) fn from_ast_foreign_reference(
        ast: &AstForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_foreign_reference_in(ast, context, table, name, keys, None)
    }

    pub(crate) fn from_ast_foreign_reference_in(
        ast: &AstForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
        pending: Option<&CreateTable>,
    ) -> Result<Self, AnalysisError> {
        #[cfg(feature = "sqlite")]
        for column in keys {
            Self::report_ast_ignored_matches(ast, context, table, column)?;
        }
        let requested = TableName::from(FullName::from_ast_name(ast.table())?);
        let explicit = Self::referenced_columns_from_ast(ast);
        let (binding, target_columns, target_valid) = if let Some(pending) = pending {
            ForeignKeyTarget::resolve_declared_reference(
                context,
                &requested,
                explicit.as_deref(),
                pending,
            )?
        } else {
            let binding = ForeignKeyTarget::resolve_relation(context, &requested)?;
            let (columns, valid) =
                binding.referenced_columns(context, &requested, explicit.as_deref())?;
            (binding, columns, valid)
        };
        let source_valid = if let Some(pending) = pending {
            pending.check_reference_columns(context, table, keys)?
        } else {
            true
        };
        let mut constraint =
            ForeignKeyConstraint::new(name, Some(table), binding.relation(), keys, &target_columns)
                .with_attributes(
                    ast.match_type().as_ref(),
                    ast.delete_action().as_ref(),
                    ast.update_action().as_ref(),
                    ast.deferrability().as_ref(),
                    ast.validity().as_ref(),
                );
        constraint.restrict_reference_application(source_valid && target_valid);
        Ok(Self::foreign(&constraint))
    }

    #[cfg(feature = "sqlite")]
    pub(crate) fn report_ast_ignored_matches(
        ast: &AstForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(), AnalysisError> {
        for name in ast.ignored_matches() {
            context.report_analysis_problem(
                AnalysisProblem::sqlite_ignores_match_option_in_fk_constraint(table, column, name),
            )?;
        }
        Ok(())
    }

    pub(crate) fn referenced_columns_from_ast(
        ast: &AstForeignReference,
    ) -> Option<Vec<ColumnName>> {
        if ast.columns().is_empty() {
            None
        } else {
            Some(
                ast.columns()
                    .iter()
                    .map(|key| ColumnName::from(key.as_str()))
                    .collect(),
            )
        }
    }
}

impl Constraint {
    pub(crate) fn from_ast_declared_foreign_key(
        ast: &AstTableConstraint,
        context: &mut impl ParseContext,
        table: &CreateTable,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = Self::from_ast_column_foreign_key(
            ast.constraint(),
            context,
            table,
            ast.name().as_deref().unwrap_or_default(),
            &Self::columns_from_ast(ast),
        )?;
        if let Some(mut constraint) = result {
            let columns = Self::delete_columns_from_ast(ast);
            let valid = table.check_reference_columns(context, table.name(), &columns)?;
            constraint.restrict_reference_application(valid);
            constraint.set_foreign_key_delete_columns(&columns);
            Ok(Some(constraint))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn delete_columns_from_ast(ast: &AstTableConstraint) -> Vec<ColumnName> {
        ast.extras()
            .delete_columns()
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect()
    }

    pub(crate) fn columns_from_ast(ast: &AstTableConstraint) -> Vec<ColumnName> {
        ast.keys()
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect()
    }

    pub(crate) fn from_ast_column_foreign_key(
        ast: &AstColumnConstraint,
        context: &mut impl ParseContext,
        table: &CreateTable,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Option<Self>, AnalysisError> {
        let AstColumnConstraint::ForeignKey(reference) = ast else {
            return Ok(None);
        };
        Self::from_ast_foreign_reference_in(
            reference,
            context,
            table.name(),
            name,
            keys,
            Some(table),
        )
        .map(Some)
    }
}
