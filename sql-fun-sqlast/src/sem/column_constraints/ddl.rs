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
    #[cfg(any(test, feature = "sqlite"))]
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
        let target = TableName::from(FullName::from_ast_name(&ast.table)?);
        #[cfg(feature = "sqlite")]
        Constraint::report_ast_ignored_matches(ast, context, table, column)?;
        let [target_column] = ast.columns.as_slice() else {
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
            ast.match_type.as_ref(),
            ast.delete_action.clone(),
            ast.update_action.clone(),
            ast.deferrability.clone(),
            ast.validity.clone(),
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
