use crate::{
    AstAndContextPair,
    ast::{Dialect, DropBehavior, DropTrigger},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, DropDependencyBehavior, DropTrigger as SemDropTrigger, ResolutionFailure,
        SemAst, TriggerIdentity, problem::TriggerProblem,
    },
};

impl SemDropTrigger {
    pub(crate) fn from_ast(
        ast: &DropTrigger,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = if context.dialect() == Dialect::sqlite() {
            Self::from_ast_sqlite(ast, context)
        } else {
            Self::from_ast_postgres(ast, context)
        };
        ResolutionFailure::finish(result, context)
    }
}

impl SemDropTrigger {
    pub(crate) fn from_ast_sqlite(
        ast: &DropTrigger,
        context: &impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let requested = FullName::from_ast_name(&ast.name)?;
        let identity = if requested.schema().is_some() {
            let identity = TriggerIdentity::sqlite(&requested);
            if let Some(definition) = context.trigger(&identity) {
                return Ok(Self::sqlite(
                    &requested,
                    Some(&identity),
                    Some(definition.target()),
                    ast.if_exists,
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
                    return Ok(Self::sqlite(
                        &requested,
                        Some(&identity),
                        Some(definition.target()),
                        ast.if_exists,
                    ));
                }
            }
            None
        };
        if !ast.if_exists {
            return Err(ResolutionFailure::trigger(&TriggerProblem::missing_name(
                &requested,
            )));
        }
        Ok(Self::sqlite(&requested, identity.as_ref(), None, true))
    }
}

impl SemDropTrigger {
    pub(crate) fn from_ast_postgres(
        ast: &DropTrigger,
        context: &impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let requested = FullName::from_ast_name(&ast.name)?;
        let requested_target =
            FullName::from_ast_name(ast.relation.as_ref().expect("PostgreSQL ON relation"))?;
        let behavior = DropDependencyBehavior::from_ast(
            &ast.behavior.expect("PostgreSQL dependency behavior"),
        );
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
            return Err(ResolutionFailure::trigger(&TriggerProblem::target(
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

impl SemDropTrigger {
    pub(crate) fn from_ast_on(
        ast: &DropTrigger,
        context: &impl ParseContext,
        requested: &FullName,
        requested_target: &FullName,
        target: &FullName,
        behavior: DropDependencyBehavior,
    ) -> Result<Self, ResolutionFailure> {
        let name = FullName::new(
            &target.catalog().cloned(),
            &target.schema().cloned(),
            requested.name(),
        );
        let identity = TriggerIdentity::postgres(target, &name);
        let found = context.trigger(&identity).is_some();
        if !found && !ast.if_exists {
            return Err(ResolutionFailure::trigger(&TriggerProblem::missing(
                &identity,
            )));
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

impl SemDropTrigger {
    pub(crate) fn apply_ast<C: ParseContext>(
        ast: &DropTrigger,
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
            SemAst::drop_trigger(&definition),
            context,
        ))
    }
}

#[cfg(test)]
mod tests;

impl DropDependencyBehavior {
    pub(crate) const fn from_ast(ast: &DropBehavior) -> Self {
        match ast {
            DropBehavior::Cascade => Self::cascade(),
            DropBehavior::Restrict => Self::restrict(),
        }
    }
}
