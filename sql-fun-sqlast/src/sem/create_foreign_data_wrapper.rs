//! Validate wrapper declarations and apply their recorded catalog effects.

use std::collections::HashSet;

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
    handler: Option<FullName>,
    validator: Option<FullName>,
    problems: Vec<AnalysisProblem>,
}

impl CreateForeignDataWrapper {
    /// Logical database-wide wrapper identifier.
    #[must_use]
    pub fn name(&self) -> &str {
        self.syntax.name()
    }

    /// Resolved zero-argument handler.
    #[must_use]
    pub const fn handler(&self) -> Option<&FullName> {
        self.handler.as_ref()
    }

    /// Resolved validator with input types `text[], oid`.
    #[must_use]
    pub const fn validator(&self) -> Option<&FullName> {
        self.validator.as_ref()
    }

    /// Declared option pairs in source order.
    #[must_use]
    pub fn options(&self) -> &[FdwOption] {
        self.syntax.options()
    }

    /// Whether the definition passed offline validation.
    #[must_use]
    pub const fn is_applicable(&self) -> bool {
        self.problems.is_empty()
    }

    /// Static validation failures recorded for this declaration.
    #[must_use]
    pub fn problems(&self) -> &[AnalysisProblem] {
        &self.problems
    }

    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        if let Some(handler) = self.handler() {
            visitor.schema_function(handler)?;
        }
        if let Some(validator) = self.validator() {
            visitor.schema_function(validator)?;
        }
        Ok(())
    }

    pub(crate) fn analyze(
        syntax: &ForeignDataWrapper,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut statement = Self {
            syntax: syntax.clone(),
            handler: None,
            validator: None,
            problems: Vec::new(),
        };
        if context.foreign_data_wrapper(syntax.name()).is_some() {
            statement.reject(
                context,
                &format!("foreign-data wrapper {:?} already exists", syntax.name()),
            )?;
        }
        statement.resolve_routines(syntax, context)?;
        let mut options = HashSet::new();
        for option in syntax.options() {
            if !options.insert(option.name()) {
                statement.reject(
                    context,
                    &format!(
                        "foreign-data wrapper option {:?} provided more than once",
                        option.name()
                    ),
                )?;
            }
        }
        Ok(statement)
    }

    fn reject(
        &mut self,
        context: &mut impl ParseContext,
        message: &str,
    ) -> Result<(), AnalysisError> {
        let problem = AnalysisProblem::foreign_data_wrapper(message);
        self.problems.push(problem.clone());
        context.report_analysis_problem(problem)
    }

    fn resolve_routines(
        &mut self,
        syntax: &ForeignDataWrapper,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let mut seen = HashSet::new();
        for clause in syntax.routines() {
            let (kind, parts) = RoutineKind::from_clause(clause);
            if !seen.insert(kind) {
                self.reject(
                    context,
                    &format!("conflicting foreign-data wrapper {} clauses", kind.label()),
                )?;
                continue;
            }
            let Some(parts) = parts else { continue };
            let name = match kind.resolve(parts, context) {
                Ok(name) => name,
                Err(message) => {
                    self.reject(context, &message)?;
                    continue;
                }
            };
            match kind {
                RoutineKind::Handler => self.handler = Some(name),
                RoutineKind::Validator => self.validator = Some(name),
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
    use std::path::Path;

    use crate::{
        ast::ObjectKind,
        context::{ForeignDataWrapperInfoRead, OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::{OwnerSpecification, SemAst},
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
        assert_eq!(
            wrapper.problems()[0].code(),
            "foreign_data_wrapper_invalid_definition"
        );
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RoutineKind {
    Handler,
    Validator,
}

impl RoutineKind {
    fn from_clause(clause: &FdwRoutineClause) -> (Self, Option<&[String]>) {
        match clause {
            FdwRoutineClause::Handler(parts) => (Self::Handler, parts.as_deref()),
            FdwRoutineClause::Validator(parts) => (Self::Validator, parts.as_deref()),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Handler => "handler",
            Self::Validator => "validator",
        }
    }

    fn resolve(self, parts: &[String], context: &impl ParseContext) -> Result<FullName, String> {
        let requested = match parts {
            [local] => FullName::from_local_name(local),
            [schema, local] => FullName::with_schema(schema, local),
            [database, schema, local]
                if context.metadata().database_name() == Some(database.as_str()) =>
            {
                FullName::with_schema(schema, local)
            }
            _ => {
                return Err(format!(
                    "cannot resolve foreign-data wrapper {} name {} in the selected database",
                    self.label(),
                    parts.join("."),
                ));
            }
        };
        let (arguments, signature) = match self {
            Self::Handler => (Vec::new(), ""),
            Self::Validator => (
                vec![
                    Some(PgBuiltInType::text().array_type()),
                    Some(TypeReference::concrete_type_ref(
                        &FullName::with_schema("pg_catalog", "oid"),
                        false,
                    )),
                ],
                "pg_catalog.text[], pg_catalog.oid",
            ),
        };
        let Some(function) = context.find_exact_function(&requested, &arguments) else {
            return Err(format!(
                "foreign-data wrapper {} requires a function matching {requested}({signature})",
                self.label(),
            ));
        };
        if self == Self::Handler {
            let expected = TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", "fdw_handler"),
                false,
            );
            if function.overload().ret_type().as_ref() != Some(&expected) {
                return Err(format!(
                    "foreign-data wrapper handler {} must return pg_catalog.fdw_handler",
                    function.name(),
                ));
            }
        }
        Ok(function.name().clone())
    }
}
