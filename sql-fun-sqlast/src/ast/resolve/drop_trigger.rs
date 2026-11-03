use crate::{
    AstAndContextPair,
    ast::{Dialect, DropBehavior, DropTrigger},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, DropDependencyBehavior, ResolutionFailure, SemAst, TriggerIdentity,
        problem::TriggerProblem,
    },
};

impl DropTrigger {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<crate::sem::DropTrigger>, AnalysisError> {
        let result = if context.dialect() == Dialect::sqlite() {
            self.resolve_sqlite(context)
        } else {
            self.resolve_postgres(context)
        };
        ResolutionFailure::finish(result, context)
    }
    fn resolve_sqlite(
        &self,
        context: &impl ParseContext,
    ) -> Result<crate::sem::DropTrigger, ResolutionFailure> {
        let requested = self.name.full_name()?;
        let identity = if requested.schema().is_some() {
            let identity = TriggerIdentity::sqlite(&requested);
            if let Some(definition) = context.trigger(&identity) {
                return Ok(crate::sem::DropTrigger::sqlite(
                    &requested,
                    Some(&identity),
                    Some(definition.target()),
                    self.if_exists,
                ));
            }
            Some(identity)
        } else {
            let mut schemas = vec![
                crate::names::SchemaName::from("temp"),
                crate::names::SchemaName::from("main"),
            ];
            for schema in context.get_search_path() {
                if !schemas
                    .iter()
                    .any(|existing| existing.as_str().eq_ignore_ascii_case(schema.as_str()))
                {
                    schemas.push(schema.clone());
                }
            }
            for schema in schemas {
                let identity = TriggerIdentity::sqlite(&requested.clone().set_schema(schema));
                if let Some(definition) = context.trigger(&identity) {
                    return Ok(crate::sem::DropTrigger::sqlite(
                        &requested,
                        Some(&identity),
                        Some(definition.target()),
                        self.if_exists,
                    ));
                }
            }
            None
        };
        if !self.if_exists {
            return Err(ResolutionFailure::trigger(&TriggerProblem::missing_name(
                &requested,
            )));
        }
        Ok(crate::sem::DropTrigger::sqlite(
            &requested,
            identity.as_ref(),
            None,
            true,
        ))
    }
    fn resolve_postgres(
        &self,
        context: &impl ParseContext,
    ) -> Result<crate::sem::DropTrigger, ResolutionFailure> {
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
                return Ok(crate::sem::DropTrigger::new(
                    &requested,
                    &requested_target,
                    None,
                    false,
                    true,
                    behavior,
                ));
            }
            return Err(ResolutionFailure::trigger(&TriggerProblem::target(
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
    ) -> Result<crate::sem::DropTrigger, ResolutionFailure> {
        let name = FullName::new(
            &target.catalog().cloned(),
            &target.schema().cloned(),
            requested.name(),
        );
        let identity = TriggerIdentity::postgres(target, &name);
        let found = context.trigger(&identity).is_some();
        if !found && !self.if_exists {
            return Err(ResolutionFailure::trigger(&TriggerProblem::missing(
                &identity,
            )));
        }
        Ok(crate::sem::DropTrigger::new(
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
            SemAst::drop_trigger(&definition),
            context,
        ))
    }
}
#[cfg(test)]
mod tests;

impl DropBehavior {
    const fn semantic(self) -> DropDependencyBehavior {
        match self {
            Self::Cascade => DropDependencyBehavior::cascade(),
            Self::Restrict => DropDependencyBehavior::restrict(),
        }
    }
}
