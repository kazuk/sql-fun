//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::{Dialect as AstDialect, ddl::Domain as AstDomain},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, CreateDomain, EngineFeatureValidation, SemAst,
        TypeCategory, TypeReference, WithTypmod,
    },
};

impl CreateDomain {
    pub(crate) fn from_ast(
        ast: &AstDomain,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod domain_tests {
    use crate::{
        ast::{StatementNode, TryFromWithTokens},
        sem::SemAst,
        test_helpers::{SynBuilder, TestParseContext, create_cache_context},
    };
    use ::sql_fun_core::IVec;

    #[test]
    fn missing_base_type_is_diagnosed_only_during_resolution() -> testresult::TestResult {
        let parser = SynBuilder::new();
        let syntax = StatementNode::try_from_with_tokens(
            &parser.parse("create domain email as absent"),
            &IVec::default(),
        )?;
        let mut context = TestParseContext::default();
        let domain = SemAst::from_ast_node(&syntax, &mut context, &mut create_cache_context())?;
        assert!(matches!(domain, SemAst::CreateDomain(_)));
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }
}

impl CreateDomain {
    pub(crate) fn from_ast_inner(
        ast: &AstDomain,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(ast.name())?;
        let base_name = FullName::from_ast_name(ast.base().name())?;
        let base = context.canonicalize_type_ref(&TypeReference::from_ast(ast.base())?);
        if base.is_none() {
            context.report_analysis_problem(AnalysisProblem::domain_base_type_not_found(
                &name,
                &base_name.to_string(),
                ast.base_span(),
            ))?;
        }
        let mut domain = Self::new(&TypeReference::from_full_name(name), &base);
        let category = if let Some(base) = context.get_type(&base_name) {
            if ast.base().array_bounds().is_empty() {
                base.category(context)
            } else if context.dialect() == AstDialect::postgres() {
                TypeCategory::try_from('A').ok()
            } else {
                None
            }
        } else {
            None
        };
        domain.set_category(category);
        domain.retain_requested_type(&WithTypmod::<TypeReference>::from_ast_declaration(
            ast.base(),
            domain.base_type(),
        )?);
        let lookup = FullName::from_optional_ast_name(ast.collation())?
            .map(|name| context.lookup_collation(&name));
        domain.validate_collation(context, lookup.as_ref())?;
        Ok(domain)
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstDomain,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !EngineFeatureValidation::domain(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let domain = Self::from_ast(ast, &mut context)?;
        let context = context.apply_create_domain(&domain)?;
        Ok(AstAndContextPair::new(
            SemAst::create_domain(&domain),
            context,
        ))
    }
}
