//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        ColumnConstraint as AstColumnConstraint, TableConstraint as AstTableConstraint,
        TryFromWithTokens,
    },
    context::ParseContext,
    names::TableName,
    sem::{
        AnalysisError, AnalysisProblem, CheckConstraint, Constraint, CreateTable,
        EngineFeatureValidation, ScalarAnalysisContext, SemScalarExpr, problem::CheckProblem,
    },
    syn::{Constraint as SynConstraint, ScanToken},
};
use ::sql_fun_core::IVec;

impl CheckConstraint {
    pub(crate) fn from_ast_table_constraint(
        ast: &AstTableConstraint,
        context: &mut impl ParseContext,
        target: &TableName,
        table: Option<&CreateTable>,
        alter: bool,
    ) -> Result<Self, AnalysisError> {
        let scope = if let Some(table) = table {
            ScalarAnalysisContext::declared_table(table, target)
        } else {
            ScalarAnalysisContext::default()
        };
        let mut check = Self::from_ast_expression(ast, context, target, &scope, alter)?;
        let name = check.name().to_owned();
        if table.is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(target))?;
            check.missing_target();
        }
        if !name.is_empty() && table.is_some_and(|table| table.has_constraint_name(&name)) {
            check.reject_duplicate_name(context)?;
        }
        Ok(check)
    }

    pub(crate) fn from_ast_expression(
        ast: &AstTableConstraint,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
        alter: bool,
    ) -> Result<Self, AnalysisError> {
        let AstColumnConstraint::Check(syntax) = &ast.constraint else {
            return AnalysisError::raise_unexpected_input("expected CHECK");
        };
        let expression = SemScalarExpr::from_ast_in(syntax, context, scope)?;
        let mut check = Self::analyze(
            context,
            target,
            ast.name.as_deref().unwrap_or_default(),
            &expression,
        )?;
        check.retain_options(
            ast.extras.no_inherit,
            ast.extras.skip_validation,
            ast.extras.enforced,
        );
        if ast.extras.deferrable || ast.extras.initially_deferred {
            check.reject(context, CheckProblem::Deferrability)?;
        }
        check.set_validated(!alter || !ast.extras.skip_validation);
        Ok(check)
    }

    pub(crate) fn from_legacy_syntax(
        context: &mut impl ParseContext,
        target: Option<&TableName>,
        syntax: &SynConstraint,
    ) -> Result<Constraint, AnalysisError> {
        let Some(target) = target else {
            return AnalysisError::raise_unexpected_none("CHECK target table");
        };
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let table = context.get_table(&canonical).cloned();
        let common =
            AstTableConstraint::try_from_with_tokens(syntax, &IVec::<ScanToken>::default())?;
        let valid = EngineFeatureValidation::table_constraint(&common, context, false)?;
        let mut check =
            Self::from_ast_table_constraint(&common, context, &canonical, table.as_ref(), true)?;
        if !valid {
            check.disallow();
        }
        Ok(Constraint::Check(check))
    }
}
