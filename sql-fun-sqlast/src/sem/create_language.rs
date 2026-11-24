use crate::{
    AstAndContextPair,
    ast::{Name, ObjectKind, create_language::CreateLanguage as Syntax},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, OwnedObjectDefinition, OwnerSpecification,
        OwnershipIdentity, OwnershipLookup, SemAst, TypeReference, ownership::RequestedIdentity,
        traversal::SemanticVisitor,
    },
};

/// An analyzed PostgreSQL procedural language declaration.
#[derive(Debug, Clone)]
pub struct CreateLanguage {
    name: FullName,
    trusted: bool,
    handler: OwnershipLookup,
    inline: Option<OwnershipLookup>,
    validator: Option<OwnershipLookup>,
    effect: CatalogEffect,
}

#[derive(Debug, Clone, Copy)]
enum CatalogEffect {
    Reject,
    Insert,
    // The ownership catalog projects only identity and owner.
    RetainIdentity,
}

impl CreateLanguage {
    /// Database-wide language name.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// Whether the declaration specifies TRUSTED.
    #[must_use]
    pub const fn is_trusted(&self) -> bool {
        self.trusted
    }

    /// Call handler lookup outcome.
    #[must_use]
    pub const fn handler(&self) -> &OwnershipLookup {
        &self.handler
    }

    /// Inline handler lookup outcome, when specified.
    #[must_use]
    pub const fn inline(&self) -> Option<&OwnershipLookup> {
        self.inline.as_ref()
    }

    /// Validator lookup outcome, when specified.
    #[must_use]
    pub const fn validator(&self) -> Option<&OwnershipLookup> {
        self.validator.as_ref()
    }

    /// Whether the declaration can update the current catalog.
    #[must_use]
    pub const fn is_applicable(&self) -> bool {
        !matches!(self.effect, CatalogEffect::Reject)
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_local_name(syntax.name());
        let handler = Self::resolve(syntax.handler(), "HANDLER", &[], context)?;
        let inline = syntax
            .inline()
            .as_ref()
            .map(|routine| {
                Self::resolve(
                    routine,
                    "INLINE",
                    &[Some(Self::pg_type("internal"))],
                    context,
                )
            })
            .transpose()?;
        let validator = syntax
            .validator()
            .as_ref()
            .map(|routine| {
                Self::resolve(routine, "VALIDATOR", &[Some(Self::pg_type("oid"))], context)
            })
            .transpose()?;
        let valid_handler = if let Some(identity) = handler.identity() {
            let valid =
                Self::handler_result(context, identity) == Some(Self::pg_type("language_handler"));
            if !valid {
                Self::report(context, format!("{identity} must return language_handler"))?;
            }
            valid
        } else {
            false
        };
        let exists = !context
            .ownership_definitions(ObjectKind::LANGUAGE, &name)
            .is_empty();
        let duplicate = !*syntax.replace() && exists;
        if duplicate {
            Self::report(context, format!("language {name} already exists"))?;
        }
        let applicable = valid_handler
            && inline
                .as_ref()
                .is_none_or(|binding| binding.identity().is_some())
            && validator
                .as_ref()
                .is_none_or(|binding| binding.identity().is_some())
            && !duplicate;
        let effect = match (applicable, exists) {
            (false, _) => CatalogEffect::Reject,
            (true, false) => CatalogEffect::Insert,
            (true, true) => CatalogEffect::RetainIdentity,
        };
        Ok(Self {
            name,
            trusted: *syntax.trusted(),
            handler,
            inline,
            validator,
            effect,
        })
    }

    fn handler_result(
        context: &impl ParseContext,
        identity: &OwnershipIdentity,
    ) -> Option<TypeReference> {
        let definition = context.exact_routine_definition(identity)?;
        definition.overloads().get_at(0)?.ret_type().clone()
    }

    fn pg_type(name: &str) -> TypeReference {
        TypeReference::from_full_name(FullName::with_schema("pg_catalog", name))
    }

    fn resolve(
        routine: &Name,
        clause: &str,
        arguments: &[Option<TypeReference>],
        context: &mut impl ParseContext,
    ) -> Result<OwnershipLookup, AnalysisError> {
        let name = FullName::from_ast_name(routine)?;
        let identity = OwnershipIdentity::with_arguments(ObjectKind::FUNCTION, &name, arguments);
        let lookup = RequestedIdentity::new(&identity, &[]).resolve_routine(context);
        if lookup.identity().is_none() {
            Self::report(context, format!("could not resolve {clause} {identity}"))?;
        }
        Ok(lookup)
    }

    fn report(context: &mut impl ParseContext, message: String) -> Result<(), AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::InvalidLanguageDeclaration(message, None))
    }

    pub(crate) fn analyze_and_apply<C: ParseContext>(
        syntax: &Syntax,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let statement = Self::analyze(syntax, &mut context)?;
        if matches!(statement.effect, CatalogEffect::Insert) {
            let identity = OwnershipIdentity::named(ObjectKind::LANGUAGE, statement.name());
            context.insert_owned_object(&OwnedObjectDefinition::new(
                &identity,
                Some(&OwnerSpecification::current_user()),
            ));
        }
        Ok(AstAndContextPair::new(
            SemAst::CreateLanguage(Box::new(statement)),
            context,
        ))
    }

    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for binding in std::iter::once(&self.handler)
            .chain(&self.inline)
            .chain(&self.validator)
        {
            if let Some(identity) = binding.identity() {
                visitor.schema_function(identity.name())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod catalog_tests {
    use crate::{
        ast::ObjectKind,
        context::{MutSourcePathWithSpan, OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::{CreateLanguage, OwnerSpecification, SemAst},
        test_helpers::{
            TestParseContext, ownership::apply_ownership_sql, schema_layer, source_span,
        },
    };

    #[test]
    fn only_applicable_declarations_publish_language_identities() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.setup_type(CreateLanguage::pg_type("language_handler"));
        let mut context = schema_layer(std::path::Path::new("language.sql"), &inventory.freeze());
        context.set_source_path_with_span(&source_span());
        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION public.handler() RETURNS language_handler LANGUAGE c AS 'symbol';",
        )?
        .into_context();
        let missing = "CREATE LANGUAGE sample HANDLER public.handler VALIDATOR public.missing;";
        let failed = apply_ownership_sql(context, missing)?;
        let SemAst::CreateLanguage(statement) = failed.ast() else {
            panic!("expected a language declaration");
        };
        assert!(!statement.is_applicable());
        assert!(statement.handler().identity().is_some());
        assert!(statement.validator().unwrap().identity().is_none());

        let name = FullName::from_local_name("sample");
        let context = failed.into_context();
        assert!(
            context
                .ownership_definitions(ObjectKind::LANGUAGE, &name)
                .is_empty()
        );
        let created = apply_ownership_sql(
            context,
            "CREATE OR REPLACE TRUSTED LANGUAGE sample HANDLER public.handler;",
        )?;
        let SemAst::CreateLanguage(statement) = created.ast() else {
            panic!("expected a language declaration");
        };
        assert!(statement.is_applicable());
        assert!(statement.is_trusted());
        let reassigned = apply_ownership_sql(
            created.into_context(),
            "ALTER LANGUAGE sample OWNER TO SESSION_USER;",
        )?;
        let mut context = schema_layer(
            std::path::Path::new("later.sql"),
            &reassigned.into_context().freeze(),
        );
        context.set_source_path_with_span(&source_span());
        let original = context.ownership_definitions(ObjectKind::LANGUAGE, &name);
        assert_eq!(original.len(), 1);
        assert_eq!(
            original[0].owner(),
            Some(&OwnerSpecification::session_user())
        );
        let duplicate =
            apply_ownership_sql(context, "CREATE LANGUAGE sample HANDLER public.handler;")?;
        let SemAst::CreateLanguage(statement) = duplicate.ast() else {
            panic!("expected a language declaration");
        };
        assert!(!statement.is_applicable());
        let failed = apply_ownership_sql(
            duplicate.into_context(),
            "CREATE OR REPLACE LANGUAGE sample HANDLER public.missing;",
        )?;
        let SemAst::CreateLanguage(statement) = failed.ast() else {
            panic!("expected a language declaration");
        };
        assert!(!statement.is_applicable());
        let replaced = apply_ownership_sql(
            failed.into_context(),
            "CREATE OR REPLACE LANGUAGE sample HANDLER public.handler;",
        )?;
        let SemAst::CreateLanguage(statement) = replaced.ast() else {
            panic!("expected a language declaration");
        };
        assert!(statement.is_applicable());
        let definitions = replaced
            .into_context()
            .ownership_definitions(ObjectKind::LANGUAGE, &name);
        assert_eq!(definitions[0].identity(), original[0].identity());
        assert_eq!(definitions[0].owner(), original[0].owner());
        Ok(())
    }
}
