use crate::{
    AstAndContextPair,
    ast::{DropBehavior, DropRule},
    context::ParseContext,
    names::{FullName, ViewName},
    sem::{
        AnalysisError, DropDependencyBehavior, DropRule as SemDropRule, ResolutionFailure,
        RuleIdentity, SemAst, problem::RuleProblem,
    },
};

impl SemDropRule {
    pub(crate) fn from_ast(
        ast: &DropRule,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = Self::from_ast_definition(ast, context);
        ResolutionFailure::finish(result, context)
    }
}

impl SemDropRule {
    pub(crate) fn from_ast_definition(
        ast: &DropRule,
        context: &impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let requested = FullName::from_ast_name(&ast.name)?;
        let requested_target = FullName::from_ast_name(&ast.relation)?;
        let behavior = match ast.behavior {
            DropBehavior::Cascade => DropDependencyBehavior::cascade(),
            DropBehavior::Restrict => DropDependencyBehavior::restrict(),
        };
        let Some(target) = context.resolve_relation_name(&requested_target) else {
            if ast.if_exists {
                return Ok(Self::new(
                    &requested,
                    &requested_target,
                    None,
                    false,
                    true,
                    behavior,
                ));
            }
            return Err(ResolutionFailure::rule(&RuleProblem::target(
                &requested_target,
            )));
        };
        Self::from_ast_on(
            ast,
            context,
            &requested,
            &requested_target,
            &target,
            behavior,
        )
    }
}

impl SemDropRule {
    pub(crate) fn from_ast_on(
        ast: &DropRule,
        context: &impl ParseContext,
        requested: &FullName,
        requested_target: &FullName,
        target: &FullName,
        behavior: DropDependencyBehavior,
    ) -> Result<Self, ResolutionFailure> {
        let identity = RuleIdentity::new(target, requested.name().as_str());
        let found = context.rule(&identity).is_some();
        if identity.name().name().as_str() == "_RETURN"
            && context
                .get_view(&ViewName::from_full_name(target))
                .is_some()
        {
            return Err(ResolutionFailure::rule(&RuleProblem::definition(
                &identity,
                "the View requires its SELECT rule; drop the View instead",
            )));
        }
        if !found && !ast.if_exists {
            return Err(ResolutionFailure::rule(&RuleProblem::missing(&identity)));
        }
        Ok(Self::new(
            requested,
            requested_target,
            Some(&identity),
            found,
            ast.if_exists,
            behavior,
        ))
    }
}

impl SemDropRule {
    pub(crate) fn apply_ast<C: ParseContext>(
        ast: &DropRule,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = Self::from_ast(ast, &mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = definition.apply(context)?;
        Ok(AstAndContextPair::new(
            SemAst::drop_rule(&definition),
            context,
        ))
    }
}

#[cfg(test)]
mod tests;
