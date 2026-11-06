//! Analysis owned by the semantic definitions in this module.

#[cfg(feature = "sqlite")]
use crate::sem::Constraint;

use crate::{
    ast::{
        ColumnConstraint as AstColumnConstraint, ForeignReference as AstForeignReference,
        TableConstraint as AstTableConstraint,
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AnalysisError, AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
        AttrImmediateColumnConstraint, CheckColumnConstraint, CheckConstraint,
        CollateColumnConstraint, ColumnConstraint, ColumnNullabilityConstraint,
        DefaultColumnConstraint, ExclusionColumnConstraint, ForeignKeyColumnConstraint,
        ForeignKeyTarget, GeneratedColumnConstraint, IdentityColumnConstraint,
        PrimaryKeyColumnConstraint, ScalarAnalysisContext, UniqueColumnConstraint,
    },
};

impl ColumnConstraint {
    #[cfg(test)]
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &AstColumnConstraint,
        mut context: TContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, &mut context, table, column);
        let constraint = context.finish_analysis(result)?;
        Ok((constraint, context))
    }
}

#[cfg(test)]
mod test_analyze_constraint {
    use crate::{
        names::ColumnName,
        sem::{ColumnConstraint, PgBuiltInType},
        test_helpers::foreign_keys as fk,
    };

    #[rstest::rstest]
    #[case::resolved("parent", true)]
    #[case::missing("absent", false)]
    fn direct_analysis_retains_completed_lookup(
        #[case] target: &str,
        #[case] resolved: bool,
    ) -> testresult::TestResult {
        let context = fk::parents();
        let syntax = fk::postgres_column_constraint(&format!(
            "ALTER TABLE a.child ADD FOREIGN KEY(id) REFERENCES {target}(id) ON UPDATE CASCADE"
        ));
        let (constraint, mut context) = ColumnConstraint::from_ast_and_report(
            &syntax,
            context,
            &fk::name("a", "child"),
            &ColumnName::from("id"),
        )?;
        let constraint = constraint.as_foreign_key().expect("foreign-key constraint");
        let expected = if resolved {
            Some(fk::name("a", "parent"))
        } else {
            None
        };
        assert_eq!(constraint.primary_key_table(), expected.as_ref());
        if resolved {
            assert_eq!(
                constraint.primary_key_column_type(),
                Some(&PgBuiltInType::int4())
            );
            assert!(context.problems().is_empty(), "{:?}", context.problems());
        } else {
            assert_eq!(context.problems()[0].code(), "table_not_found");
        }
        fk::reversed_path(&mut context);
        assert_eq!(constraint.primary_key_table(), expected.as_ref());
        Ok(())
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_analyze_constraint_sqlite {
    use crate::{
        names::{ColumnName, FullName},
        sem::ColumnConstraint,
        test_helpers::foreign_keys as fk,
    };

    #[test]
    fn direct_sqlite_analysis_retains_the_canonical_target() -> testresult::TestResult {
        let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE Parent(id PRIMARY KEY)");
        let syntax = fk::sqlite_column_constraint("CREATE TABLE child(id REFERENCES PARENT(id))");
        let (result, context) = ColumnConstraint::from_ast_and_report(
            &syntax,
            context,
            &fk::name("main", "child"),
            &ColumnName::from("id"),
        )?;
        assert_eq!(
            result
                .as_foreign_key()
                .unwrap()
                .primary_key_table()
                .unwrap()
                .full_name(),
            &FullName::with_schema("main", "parent")
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        Ok(())
    }
}

impl ColumnConstraint {
    pub(crate) fn from_ast(
        ast: &AstColumnConstraint,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<Self, AnalysisError> {
        Ok(match ast {
            AstColumnConstraint::Enforcement(_) => {
                unreachable!("versioned enforcement is checked before resolution")
            }
            AstColumnConstraint::Nullability(not_null, conflict, validity) => {
                Self::nullability(ColumnNullabilityConstraint::from_ast_parts(
                    context, table, column, *not_null, conflict, validity,
                ))
            }
            AstColumnConstraint::Default(expression) => Self::default(
                DefaultColumnConstraint::from_ast_expression(context, table, column, expression)?,
            ),
            AstColumnConstraint::Check(_) => {
                Self::from_ast_standalone_check(ast, context, table, column)?
            }
            AstColumnConstraint::PrimaryKey(conflict, deferrable) => {
                Self::primary_key(PrimaryKeyColumnConstraint::from_ast_parts(
                    context, table, column, conflict, deferrable,
                ))
            }
            AstColumnConstraint::Unique(conflict) => Self::unique(
                UniqueColumnConstraint::from_ast_conflict(context, table, column, conflict),
            ),
            AstColumnConstraint::Identity(mode) => Self::identity(
                IdentityColumnConstraint::from_ast_mode(context, table, column, mode),
            ),
            AstColumnConstraint::Generated(expression, mode) => {
                Self::generated(GeneratedColumnConstraint::from_ast_expression(
                    context, table, column, expression, mode,
                )?)
            }
            AstColumnConstraint::ForeignKey(reference) => {
                Self::from_ast_foreign_reference(reference, context, table, column)?
            }
            AstColumnConstraint::Exclusion => Self::exclusion(
                ExclusionColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Deferrability(value) => Self::attr_deferrability(
                AttrDeferrabilityColumnConstraint::from_ast_value(context, table, column, *value),
            ),
            AstColumnConstraint::Deferred => Self::attr_deferred(
                AttrDeferredColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Immediate => Self::attr_immediate(
                AttrImmediateColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Collate(name) => Self::collate(
                CollateColumnConstraint::from_ast_name(context, table, column, name)?,
            ),
        })
    }

    pub(crate) fn from_ast_foreign_reference(
        ast: &AstForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<Self, AnalysisError> {
        let target = TableName::from(FullName::from_ast_name(ast.table())?);
        #[cfg(feature = "sqlite")]
        Constraint::report_ast_ignored_matches(ast, context, table, column)?;
        let [target_column] = ast.columns().as_slice() else {
            return AnalysisError::raise_unexpected_input("foreign key target column count != 1");
        };
        let target_column = ColumnName::from(target_column.as_str());
        let (binding, target_type) =
            ForeignKeyTarget::resolve_column(context, &target, &target_column)?;
        Ok(Self::foreign_key(ForeignKeyColumnConstraint::new(
            table,
            column,
            binding.relation(),
            &target_column,
            target_type.as_ref(),
            ast.match_type().as_ref(),
            ast.delete_action().clone(),
            ast.update_action().clone(),
            ast.deferrability().clone(),
            ast.validity().clone(),
        )))
    }
}

impl ColumnConstraint {
    pub(crate) fn from_ast_standalone_check(
        ast: &AstColumnConstraint,
        context: &mut impl ParseContext,
        target: &TableName,
        column: &ColumnName,
    ) -> Result<Self, AnalysisError> {
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let definition = context.get_table(&canonical).cloned();
        let scope = if let Some(definition) = &definition {
            ScalarAnalysisContext::declared_table(definition, &canonical)
        } else {
            ScalarAnalysisContext::default()
        };
        let syntax = AstTableConstraint::unnamed_check(ast);
        let check =
            CheckConstraint::from_ast_expression(&syntax, context, &canonical, &scope, false)?;
        Ok(Self::check(CheckColumnConstraint::from_analyzed(
            &canonical, column, &check,
        )))
    }
}
