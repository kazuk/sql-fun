//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{Column as AstColumn, TableConstraint as AstTableConstraint},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, CheckColumnConstraint, CheckConstraint, EngineFeatureValidation,
        ScalarAnalysisContext,
    },
};

impl CheckColumnConstraint {
    pub(crate) fn from_ast_legacy_inline<TContext: ParseContext>(
        ast: &AstTableConstraint,
        mut context: TContext,
        target: &TableName,
        column: &ColumnName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let table = context.get_table(&canonical).cloned();
        let scope = if let Some(table) = &table {
            ScalarAnalysisContext::declared_table(table, &canonical)
        } else {
            ScalarAnalysisContext::default()
        };
        let valid = EngineFeatureValidation::table_constraint(ast, &mut context, true)?;
        let result =
            CheckConstraint::from_ast_expression(ast, &mut context, &canonical, &scope, false);
        let mut check = context.finish_analysis(result)?;
        if !valid {
            check.disallow();
        }
        Ok((Self::from_analyzed(&canonical, column, &check), context))
    }
}

impl CheckColumnConstraint {
    pub(crate) fn has_ast_column_checks(ast: &AstColumn) -> bool {
        ast.constraints.iter().any(AstTableConstraint::is_check)
    }
}
