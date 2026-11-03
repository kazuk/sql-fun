use crate::{
    AstAndContextPair,
    ast::{DropBehavior, DropRule},
    context::ParseContext,
    names::{FullName, ViewName},
    sem::{
        AnalysisError, DropDependencyBehavior, ResolutionFailure, RuleIdentity, SemAst,
        problem::RuleProblem,
    },
};

impl DropRule {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<crate::sem::DropRule>, AnalysisError> {
        let result = self.resolve_definition(context);
        ResolutionFailure::finish(result, context)
    }
    fn resolve_definition(
        &self,
        context: &impl ParseContext,
    ) -> Result<crate::sem::DropRule, ResolutionFailure> {
        let requested = self.name.full_name()?;
        let requested_target = self.relation.full_name()?;
        let behavior = match self.behavior {
            DropBehavior::Cascade => DropDependencyBehavior::cascade(),
            DropBehavior::Restrict => DropDependencyBehavior::restrict(),
        };
        let Some(target) = context.resolve_relation_name(&requested_target) else {
            if self.if_exists {
                return Ok(crate::sem::DropRule::new(
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
        self.resolve_on(context, &requested, &requested_target, &target, behavior)
    }
    fn resolve_on(
        &self,
        context: &impl ParseContext,
        requested: &FullName,
        requested_target: &FullName,
        target: &FullName,
        behavior: DropDependencyBehavior,
    ) -> Result<crate::sem::DropRule, ResolutionFailure> {
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
        if !found && !self.if_exists {
            return Err(ResolutionFailure::rule(&RuleProblem::missing(&identity)));
        }
        Ok(crate::sem::DropRule::new(
            requested,
            requested_target,
            Some(&identity),
            found,
            self.if_exists,
            behavior,
        ))
    }
    pub(crate) fn resolve_and_apply<C: ParseContext>(
        &self,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = self.resolve(&mut context)? else {
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
