//! Validate wrapper declarations and apply their recorded catalog effects.

use std::collections::HashSet;

use ::sql_fun_core::analysis_problem::{
    FdwRoutineKind, ForeignDataWrapperFailure, ForeignDataWrapperProblem,
};

use crate::{
    AstAndContextPair,
    ast::{FdwOption, FdwRoutineClause, ForeignDataWrapper, ObjectKind},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, OwnedObjectDefinition, OwnerSpecification,
        OwnershipIdentity, PgBuiltInType, SemAst, TypeReference, traversal::SemanticVisitor,
    },
};

/// Analyzed PostgreSQL foreign-data wrapper declaration.
#[derive(Clone, Debug)]
pub struct CreateForeignDataWrapper {
    syntax: ForeignDataWrapper,
    routines: Vec<FdwRoutineBinding>,
    problems: Vec<ForeignDataWrapperProblem>,
    applicable: bool,
}

impl CreateForeignDataWrapper {
    /// Logical database-wide wrapper identifier.
    #[must_use]
    pub fn name(&self) -> &str {
        self.syntax.name()
    }

    /// Resolved zero-argument handler.
    #[must_use]
    pub fn handler(&self) -> Option<&FullName> {
        self.routine_name(FdwRoutineKind::Handler)
    }

    /// Resolved validator with input types `text[], oid`.
    #[must_use]
    pub fn validator(&self) -> Option<&FullName> {
        self.routine_name(FdwRoutineKind::Validator)
    }

    /// Completed lookup outcomes for the routine clauses in source order.
    #[must_use]
    pub fn routines(&self) -> &[FdwRoutineBinding] {
        &self.routines
    }

    fn routine_name(&self, kind: FdwRoutineKind) -> Option<&FullName> {
        self.routines
            .iter()
            .find(|binding| binding.kind() == kind)?
            .outcome()
            .resolved_name()
    }

    /// Declared option pairs in source order.
    #[must_use]
    pub fn options(&self) -> &[FdwOption] {
        self.syntax.options()
    }

    /// Whether the definition passed offline validation.
    #[must_use]
    pub const fn is_applicable(&self) -> bool {
        self.applicable
    }

    /// Static validation failures recorded for this declaration.
    #[must_use]
    pub fn problems(&self) -> &[ForeignDataWrapperProblem] {
        &self.problems
    }

    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for binding in self.routines() {
            if let Some(name) = binding.outcome().resolved_name() {
                visitor.schema_function(name)?;
            }
        }
        Ok(())
    }

    pub(crate) fn analyze(
        syntax: &ForeignDataWrapper,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut statement = Self {
            syntax: syntax.clone(),
            routines: Vec::new(),
            problems: Vec::new(),
            applicable: true,
        };
        if context.foreign_data_wrapper(syntax.name()).is_some() {
            statement.reject(context, &ForeignDataWrapperFailure::DuplicateWrapper)?;
        }
        statement.resolve_routines(syntax, context)?;
        let mut options = HashSet::new();
        for option in syntax.options() {
            if !options.insert(option.name()) {
                statement.reject(
                    context,
                    &ForeignDataWrapperFailure::DuplicateOption(option.name().to_owned()),
                )?;
            }
        }
        Ok(statement)
    }

    fn reject(
        &mut self,
        context: &mut impl ParseContext,
        failure: &ForeignDataWrapperFailure,
    ) -> Result<(), AnalysisError> {
        self.applicable = false;
        let problem = ForeignDataWrapperProblem::new(self.name(), failure);
        context.report_analysis_problem(AnalysisProblem::foreign_data_wrapper(&problem))?;
        self.problems.push(problem);
        Ok(())
    }

    fn resolve_routines(
        &mut self,
        syntax: &ForeignDataWrapper,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let mut seen = HashSet::new();
        for clause in syntax.routines() {
            let (kind, parts) = match clause {
                FdwRoutineClause::Handler(parts) => (FdwRoutineKind::Handler, parts.as_deref()),
                FdwRoutineClause::Validator(parts) => (FdwRoutineKind::Validator, parts.as_deref()),
            };
            if !seen.insert(kind) {
                self.reject(context, &ForeignDataWrapperFailure::ConflictingClause(kind))?;
            }
            let (outcome, failure) = FdwRoutineOutcome::analyze(kind, parts, context);
            self.routines.push(FdwRoutineBinding { kind, outcome });
            if let Some(failure) = failure {
                self.reject(context, &failure)?;
            }
        }
        Ok(())
    }

    pub(crate) fn analyze_and_apply<C: ParseContext>(
        syntax: &ForeignDataWrapper,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let definition = Self::analyze(syntax, &mut context)?;
        if definition.is_applicable() {
            let identity = OwnershipIdentity::named(
                ObjectKind::FOREIGN_DATA_WRAPPER,
                &FullName::from_local_name(definition.name()),
            );
            context.insert_owned_object(&OwnedObjectDefinition::new(
                &identity,
                Some(&OwnerSpecification::current_user()),
            ));
            context = context.register_foreign_data_wrapper(&definition);
        }
        Ok(AstAndContextPair::new(
            SemAst::CreateForeignDataWrapper(Box::new(definition)),
            context,
        ))
    }
}

#[cfg(test)]
mod test_catalog_application {
    use ::sql_fun_core::analysis_problem::ForeignDataWrapperFailure;
    use std::path::Path;

    use crate::{
        ast::ObjectKind,
        context::{ForeignDataWrapperInfoRead, OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::{FdwRoutineOutcome, OwnerSpecification, SemAst},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer,
        },
    };

    #[test]
    fn keeps_wrapper_options_and_ownership_across_layers() -> testresult::TestResult {
        let created = apply_ownership_sql(
            ownership_context(),
            r#"CREATE FOREIGN DATA WRAPPER "a.b" OPTIONS ("Mode" 'read', empty '')"#,
        )?;
        let base = created.into_context().freeze();
        let layer = schema_layer(Path::new("fdw-child.sql"), &base);
        let wrapper = layer.foreign_data_wrapper("a.b").unwrap();
        assert_eq!(wrapper.options()[0].name(), "Mode");
        assert_eq!(wrapper.options()[0].value(), "read");
        assert_eq!(wrapper.options()[1].value(), "");
        assert_eq!(
            layer.ownership_definitions(
                ObjectKind::FOREIGN_DATA_WRAPPER,
                &FullName::from_local_name("a.b")
            )[0]
            .owner(),
            Some(&OwnerSpecification::current_user()),
        );
        let result =
            apply_ownership_sql(layer, r#"ALTER FOREIGN DATA WRAPPER "a.b" OWNER TO alice"#)?;
        assert!(
            result
                .ast()
                .ownership_change()
                .unwrap()
                .applied_identity()
                .is_some()
        );
        assert_eq!(
            result.context().ownership_definitions(
                ObjectKind::FOREIGN_DATA_WRAPPER,
                &FullName::from_local_name("a.b")
            )[0]
            .owner()
            .unwrap()
            .resolved_role()
            .unwrap()
            .name(),
            "alice",
        );
        let result = apply_ownership_sql(
            result.into_context(),
            r#"GRANT USAGE ON FOREIGN DATA WRAPPER "a.b" TO bob"#,
        )?;
        let result = apply_ownership_sql(
            result.into_context(),
            r#"REVOKE USAGE ON FOREIGN DATA WRAPPER "a.b" FROM bob"#,
        )?;
        let definition = &result.context().ownership_definitions(
            ObjectKind::FOREIGN_DATA_WRAPPER,
            &FullName::from_local_name("a.b"),
        )[0];
        let declarations = definition.privilege_declarations();
        assert_eq!(declarations.len(), 2);
        assert_eq!(declarations[0].privileges()[0].name(), "usage");
        assert_ne!(declarations[0].operation(), declarations[1].operation());
        assert_eq!(
            base.ownership_definitions(
                ObjectKind::FOREIGN_DATA_WRAPPER,
                &FullName::from_local_name("a.b")
            )[0]
            .owner(),
            Some(&OwnerSpecification::current_user()),
        );
        Ok(())
    }

    #[test]
    fn retains_each_lookup_even_when_the_declaration_is_rejected() -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.bad_handler() RETURNS text LANGUAGE sql AS 'SELECT NULL'",
        )?
        .into_context();
        let result = apply_ownership_sql(
            context,
            "CREATE FOREIGN DATA WRAPPER rejected NO HANDLER HANDLER public.bad_handler VALIDATOR missing",
        )?;
        let SemAst::CreateForeignDataWrapper(wrapper) = result.ast() else {
            panic!("expected a retained wrapper declaration");
        };
        assert!(!wrapper.is_applicable());
        let [explicit_none, invalid_handler, missing_validator] = wrapper.routines() else {
            panic!("expected an outcome for every clause");
        };
        assert!(matches!(
            explicit_none.outcome(),
            FdwRoutineOutcome::ExplicitNone
        ));
        assert_eq!(
            invalid_handler.outcome().resolved_name(),
            Some(&FullName::with_schema("public", "bad_handler")),
        );
        assert!(matches!(
            missing_validator.outcome(),
            FdwRoutineOutcome::NotFound(name) if name == &FullName::from_local_name("missing")
        ));
        assert!(matches!(
            wrapper.problems()[1].failure(),
            ForeignDataWrapperFailure::InvalidHandlerReturn(_, Some(_))
        ));
        assert!(result.context().foreign_data_wrapper("rejected").is_none());
        Ok(())
    }

    #[test]
    fn rejected_wrapper_does_not_publish_even_without_delivered_diagnostics()
    -> testresult::TestResult {
        let mut context = ownership_context();
        context.begin_parameter_type_analysis();
        let result = apply_ownership_sql(
            context,
            "CREATE FOREIGN DATA WRAPPER retry OPTIONS (a 'first', a 'second')",
        )?;
        let SemAst::CreateForeignDataWrapper(wrapper) = result.ast() else {
            panic!("expected a retained wrapper declaration");
        };
        assert!(!wrapper.is_applicable());
        assert!(matches!(
            wrapper.problems()[0].failure(),
            ForeignDataWrapperFailure::DuplicateOption(option) if option == "a"
        ));
        let mut context = result.into_context();
        assert!(context.problems().is_empty());
        let suppressed = context.analysis_state_mut().take_problems();
        assert_eq!(suppressed.len(), 1);
        assert!(context.foreign_data_wrapper("retry").is_none());
        assert!(
            context
                .ownership_definitions(
                    ObjectKind::FOREIGN_DATA_WRAPPER,
                    &FullName::from_local_name("retry")
                )
                .is_empty()
        );
        let next = apply_ownership_sql(context, "CREATE FOREIGN DATA WRAPPER retry")?;
        assert!(next.context().foreign_data_wrapper("retry").is_some());
        Ok(())
    }
}

/// A routine clause and its completed semantic lookup.
#[derive(Clone, Debug)]
pub struct FdwRoutineBinding {
    kind: FdwRoutineKind,
    outcome: FdwRoutineOutcome,
}

impl FdwRoutineBinding {
    /// The role of this clause's routine.
    #[must_use]
    pub const fn kind(&self) -> FdwRoutineKind {
        self.kind
    }

    /// The result of looking up this clause's routine.
    #[must_use]
    pub const fn outcome(&self) -> &FdwRoutineOutcome {
        &self.outcome
    }
}

/// Completed lookup for a handler or validator clause. An omitted clause has no binding.
#[derive(Clone, Debug)]
pub enum FdwRoutineOutcome {
    /// An explicit `NO HANDLER` or `NO VALIDATOR`.
    ExplicitNone,
    /// The identifier components could not be resolved in the selected database.
    InvalidName(Vec<String>),
    /// No visible function matched the requested identifier and this clause's signature.
    NotFound(FullName),
    /// The exact qualified function selected by lookup, even if validation rejected it.
    Resolved(FullName),
}

impl FdwRoutineOutcome {
    /// Return the qualified binding when exact lookup succeeded.
    #[must_use]
    pub const fn resolved_name(&self) -> Option<&FullName> {
        if let Self::Resolved(name) = self {
            Some(name)
        } else {
            None
        }
    }

    fn analyze(
        kind: FdwRoutineKind,
        parts: Option<&[String]>,
        context: &impl ParseContext,
    ) -> (Self, Option<ForeignDataWrapperFailure>) {
        let Some(parts) = parts else {
            return (Self::ExplicitNone, None);
        };
        let requested = match parts {
            [local] => FullName::from_local_name(local),
            [schema, local] => FullName::with_schema(schema, local),
            [database, schema, local]
                if context.metadata().database_name() == Some(database.as_str()) =>
            {
                FullName::with_schema(schema, local)
            }
            _ => {
                return (
                    Self::InvalidName(parts.to_vec()),
                    Some(ForeignDataWrapperFailure::InvalidRoutineName(
                        kind,
                        parts.to_vec(),
                    )),
                );
            }
        };
        let arguments = match kind {
            FdwRoutineKind::Handler => Vec::new(),
            FdwRoutineKind::Validator => vec![
                Some(PgBuiltInType::text().array_type()),
                Some(TypeReference::concrete_type_ref(
                    &FullName::with_schema("pg_catalog", "oid"),
                    false,
                )),
            ],
        };
        let Some(function) = context.find_exact_function(&requested, &arguments) else {
            let failure = ForeignDataWrapperFailure::RoutineNotFound(kind, (&requested).into());
            return (Self::NotFound(requested), Some(failure));
        };
        let failure = Self::validate(
            kind,
            function.name(),
            function.overload().ret_type().as_ref(),
        );
        (Self::Resolved(function.name().clone()), failure)
    }

    fn validate(
        kind: FdwRoutineKind,
        name: &FullName,
        return_type: Option<&TypeReference>,
    ) -> Option<ForeignDataWrapperFailure> {
        let expected = TypeReference::concrete_type_ref(
            &FullName::with_schema("pg_catalog", "fdw_handler"),
            false,
        );
        if kind == FdwRoutineKind::Handler && return_type != Some(&expected) {
            Some(ForeignDataWrapperFailure::InvalidHandlerReturn(
                name.into(),
                return_type.map(Into::into),
            ))
        } else {
            None
        }
    }
}
