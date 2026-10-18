use crate::ast::{Dialect, DropBehavior, DropTrigger};
use crate::sem::problem::TriggerProblem;
use crate::sem::{self, AnalysisError, FullName, ParseContext, SemAst, TriggerIdentity};

impl DropTrigger {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<sem::DropTrigger, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            todo!("SQLite DROP TRIGGER semantic resolution: #555");
        }
        let result = self.resolve_postgres(context);
        if let Err(AnalysisError::Trigger(problem)) = &result {
            context.report_analysis_problem(sem::AnalysisProblem::trigger(problem))?;
        }
        result
    }
    fn resolve_postgres(
        &self,
        context: &impl ParseContext,
    ) -> Result<sem::DropTrigger, AnalysisError> {
        let requested = self.name.full_name()?;
        let requested_target = self
            .relation
            .as_ref()
            .expect("PostgreSQL ON relation")
            .full_name()?;
        let behavior = self
            .behavior
            .expect("PostgreSQL dependency behavior")
            .semantic();
        let Some(target) = context.resolve_relation_name(&requested_target) else {
            if self.if_exists {
                return Ok(sem::DropTrigger::new(
                    &requested,
                    &requested_target,
                    None,
                    false,
                    true,
                    behavior,
                ));
            }
            return Err(AnalysisError::trigger(&TriggerProblem::target(
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
        behavior: sem::DropDependencyBehavior,
    ) -> Result<sem::DropTrigger, AnalysisError> {
        let name = FullName::new(
            &target.catalog().cloned(),
            &target.schema().cloned(),
            requested.name(),
        );
        let identity = TriggerIdentity::postgres(target, &name);
        let found = context.trigger(&identity).is_some();
        if !found && !self.if_exists {
            return Err(AnalysisError::trigger(&TriggerProblem::missing(&identity)));
        }
        Ok(sem::DropTrigger::new(
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
    ) -> Result<crate::AstAndContextPair<C>, AnalysisError> {
        let statement = self.resolve(&mut context)?;
        let context = statement.apply(context)?;
        Ok(crate::AstAndContextPair::new(
            SemAst::drop_trigger(&statement),
            context,
        ))
    }
}
#[cfg(test)]
mod tests;

impl DropBehavior {
    const fn semantic(self) -> sem::DropDependencyBehavior {
        match self {
            Self::Cascade => sem::DropDependencyBehavior::cascade(),
            Self::Restrict => sem::DropDependencyBehavior::restrict(),
        }
    }
}
