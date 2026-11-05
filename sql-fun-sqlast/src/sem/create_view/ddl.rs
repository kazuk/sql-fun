//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair, CacheContext,
    ast::{Expr as AstExpr, SyntaxValue as AstSyntaxValue, View as AstView},
    context::ParseContext,
    names::{FullName, ViewName},
    sem::{
        AnalysisError, AnalysisProblem, CreateRule, CreateView, FromClause, Kind,
        ScalarAnalysisContext, ScalarConstExpr, SelectStatement, SemAst, SemScalarExpr,
        ViewCheckOption, ViewColumnLineage, ViewOption, WithClause, problem::ViewProblem,
    },
};

impl CreateView {
    pub(crate) fn from_ast_options(
        ast: &AstView,
        context: &mut impl ParseContext,
    ) -> Result<Vec<ViewOption>, AnalysisError> {
        let mut options = Vec::new();
        for option in &ast.options {
            let value = match &option.value {
                None => None,
                Some(AstSyntaxValue::Expression(AstExpr::Column(name))) => Some(
                    SemScalarExpr::literal(&ScalarConstExpr::string(&name.0.join("."))),
                ),
                Some(AstSyntaxValue::Expression(expression)) => Some(SemScalarExpr::from_ast(
                    expression,
                    context,
                    &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
                )?),
                Some(AstSyntaxValue::Type(_)) => {
                    return AnalysisError::raise_unexpected_input("View option type");
                }
            };
            options.push(ViewOption::new(
                option.namespace.as_deref(),
                &option.name,
                value.as_ref(),
            ));
        }
        Ok(options)
    }
}

impl CreateView {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstView,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut context = context;
        let requested = FullName::from_ast_name(&ast.name)?;
        if requested.schema().is_none() && !ast.temporary && context.get_default_schema().is_none()
        {
            context.report_analysis_problem(AnalysisProblem::view(
                &ViewProblem::invalid_target(&requested),
            ))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let name = context.creation_name(&requested, ast.temporary)?;
        let kind = if ast.materialized {
            Kind::materialized_view()
        } else {
            Kind::view()
        };
        if !context.permits_relation_creation(&name, kind, ast.if_not_exists, ast.replace)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let outcome = SelectStatement::from_ast(&ast.query, &mut context, cache);
        let mut query = outcome?;
        query.rename_columns(&ast.aliases);
        let lineage = ViewColumnLineage::from_ast_query(&ast.query, &context, &query)?;
        let options = Self::from_ast_options(ast, &mut context)?;
        let mut view = Self::new(&name, &query, &[], &None, ast.materialized)
            .with_if_not_exists(ast.if_not_exists)
            .with_replace(ast.replace)
            .with_temporary(ast.temporary)
            .with_column_lineage(&lineage)
            .with_options(&options, ViewCheckOption::from_ast(&ast.check_option));
        let replacement_rule = if ast.replace {
            if let Some(existing) = context.get_view(&ViewName::from_full_name(&name))
                && let Some(references) = existing.rules()
            {
                view.retain_rule_references(references);
            }
            CreateRule::replace_view_definition(&context, &name, &query)?
        } else {
            None
        };
        let context = context.apply_create_view(&view)?;
        let context = if let Some(rule) = replacement_rule {
            context.register_rule(&rule)?
        } else {
            context
        };
        Ok(AstAndContextPair::new(
            SemAst::from_create_view(&view),
            context,
        ))
    }
}
