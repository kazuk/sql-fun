use crate::ast::{DropBehavior, DropRule};
use crate::sem::problem::RuleProblem;
use crate::sem::{self, AnalysisError, FullName, ParseContext, RuleIdentity, SemAst};

impl DropRule {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<sem::DropRule, AnalysisError> {
        let result = self.resolve_definition(context);
        if let Err(AnalysisError::Rule(problem)) = &result {
            context.report_analysis_problem(sem::AnalysisProblem::rule(problem))?;
        }
        result
    }
    fn resolve_definition(
        &self,
        context: &impl ParseContext,
    ) -> Result<sem::DropRule, AnalysisError> {
        let requested = self.name.full_name()?;
        let requested_target = self.relation.full_name()?;
        let cascade = matches!(self.behavior, DropBehavior::Cascade);
        let Some(target) = context.resolve_relation_name(&requested_target) else {
            if self.if_exists {
                return Ok(sem::DropRule::new(
                    &requested,
                    &requested_target,
                    None,
                    false,
                    true,
                    cascade,
                ));
            }
            return Err(AnalysisError::rule(&RuleProblem::target(&requested_target)));
        };
        self.resolve_on(context, &requested, &requested_target, &target, cascade)
    }
    fn resolve_on(
        &self,
        context: &impl ParseContext,
        requested: &FullName,
        requested_target: &FullName,
        target: &FullName,
        cascade: bool,
    ) -> Result<sem::DropRule, AnalysisError> {
        let identity = RuleIdentity::new(target, requested.name().as_str());
        let found = context.rule(&identity).is_some();
        if identity.name().name().as_str() == "_RETURN" && context.get_view_impl(target).is_some() {
            return Err(AnalysisError::rule(&RuleProblem::definition(
                &identity,
                "the View requires its SELECT rule; drop the View instead",
            )));
        }
        if !found && !self.if_exists {
            return Err(AnalysisError::rule(&RuleProblem::missing(&identity)));
        }
        Ok(sem::DropRule::new(
            requested,
            requested_target,
            Some(&identity),
            found,
            self.if_exists,
            cascade,
        ))
    }
    pub(crate) fn resolve_and_apply<C: ParseContext>(
        &self,
        mut context: C,
    ) -> Result<crate::AstAndContextPair<C>, AnalysisError> {
        let statement = self.resolve(&mut context)?;
        let context = statement.apply(context)?;
        Ok(crate::AstAndContextPair::new(
            SemAst::drop_rule(&statement),
            context,
        ))
    }
}

#[cfg(test)]
mod tests;
