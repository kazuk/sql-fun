//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{ObjectKind, Role as AstRole, TypeName as AstTypeName},
    context::ParseContext,
    names::{FullName, SchemaName},
    sem::{
        AnalysisError, AnalysisProblem, OwnerSpecification, OwnershipIdentity, OwnershipLookup,
        TypeReference,
    },
};

impl OwnershipIdentity {
    pub(crate) fn argument_type_from_ast(
        ast: &AstTypeName,
        context: &mut impl ParseContext,
    ) -> Result<Option<TypeReference>, AnalysisError> {
        let requested = FullName::from_ast_name(ast.name())?;
        Ok(context
            .resolve_type_name(&requested)
            .map(|name| TypeReference::concrete_type_ref(&name, !ast.array_bounds().is_empty())))
    }
}

impl OwnerSpecification {
    pub(crate) fn from_ast(
        ast: &AstRole,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        match ast.kind() {
            1 => Self::from_ast_named_role(ast, context),
            2 => Ok(Self::current_role()),
            3 => Ok(Self::current_user()),
            4 => Ok(Self::session_user()),
            _ => AnalysisError::raise_unexpected_input("invalid ALTER OWNER role specification"),
        }
    }
}

#[cfg(test)]
mod test_session_owners_remain_symbolic {
    use crate::{
        context::OwnershipInfoRead,
        sem::OwnerSpecification,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[rstest::rstest]
    #[case::current_role("CURRENT_ROLE", OwnerSpecification::current_role())]
    #[case::current_user("CURRENT_USER", OwnerSpecification::current_user())]
    #[case::session_user("SESSION_USER", OwnerSpecification::session_user())]
    fn session_owners_remain_symbolic(
        #[case] sql: &str,
        #[case] expected: OwnerSpecification,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), "CREATE SCHEMA app")?.into_context();
        let result = apply_ownership_sql(context, &format!("ALTER SCHEMA app OWNER TO {sql}"))?;
        let owner = result.ast().ownership_change().unwrap().owner();
        assert_eq!(owner, &expected);
        assert!(owner.resolved_role().is_none());
        assert!(owner.name().is_none());
        assert_eq!(
            result.context().schema_definition("app").unwrap().owner(),
            Some(&expected)
        );
        Ok(())
    }
}

impl OwnerSpecification {
    fn from_ast_named_role(
        ast: &AstRole,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        if let Some(role) = context.role_by_name(ast.name()) {
            Ok(Self::role(role))
        } else {
            context
                .report_analysis_problem(AnalysisProblem::ownership_role_not_found(ast.name()))?;
            Ok(Self::named(ast.name()))
        }
    }
}

#[cfg(test)]
mod test_failure_to_accept_a_diagnostic_aborts_before_any_owner_update {
    use crate::{
        ast::ObjectKind,
        context::{OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::{OwnedObjectDefinition, OwnerSpecification, OwnershipIdentity, SemAst},
        test_helpers::{TestParseContext, create_cache_context, ownership::ownership_statement},
    };

    #[test]
    fn failure_to_accept_a_diagnostic_aborts_before_any_owner_update() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let identity =
            OwnershipIdentity::named(ObjectKind::from(37), &FullName::from_local_name("app"));
        context.insert_owned_object(&OwnedObjectDefinition::new(
            &identity,
            Some(&OwnerSpecification::session_user()),
        ));
        context.reject_diagnostics_after(0);
        let result = SemAst::from_ast_node(
            &ownership_statement("ALTER SCHEMA app OWNER TO missing")?,
            &mut context,
            &mut create_cache_context(),
        );
        assert!(result.is_err());
        assert_eq!(
            context.ownership_definitions(ObjectKind::from(37), identity.name())[0].owner(),
            Some(&OwnerSpecification::session_user())
        );
        Ok(())
    }
}

impl OwnershipIdentity {
    pub(crate) fn resolve(&self, context: &impl ParseContext) -> OwnershipLookup {
        if !Self::valid_owner_kind(self.kind()) {
            return OwnershipLookup::invalid_kind(self);
        }
        let names = self.qualified_candidate_names(context);
        if self.needs_visible_routine_resolution() {
            return self.resolve_visible_routines(context, &names);
        }
        for name in names {
            if let Some(outcome) = self.with_name(&name).resolve_qualified_target(context) {
                return outcome;
            }
        }
        OwnershipLookup::not_found(self)
    }
}

#[cfg(test)]
mod test_signatures_resolve_through_search_path_and_preserve_quoted_names {
    use crate::{
        context::{OwnershipInfoRead, ParseContext},
        names::{FullName, SchemaName},
        sem::Role,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[test]
    fn signatures_resolve_through_search_path_and_preserve_quoted_names() -> testresult::TestResult
    {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION first.f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let mut context = apply_ownership_sql(
            context,
            "CREATE FUNCTION second.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        context.set_search_path(&[SchemaName::from("first"), SchemaName::from("second")]);
        let result = apply_ownership_sql(context, "ALTER FUNCTION f(integer) OWNER TO alice")?;
        assert_eq!(
            result
                .ast()
                .ownership_change()
                .unwrap()
                .target()
                .identity()
                .unwrap()
                .name(),
            &FullName::with_schema("second", "f")
        );
        let context =
            apply_ownership_sql(result.into_context(), "CREATE SCHEMA \"a.b\"")?.into_context();
        let result =
            apply_ownership_sql(context, "ALTER SCHEMA \"a.b\" OWNER TO \"Quoted Owner\"")?;
        assert_eq!(
            result
                .ast()
                .ownership_change()
                .unwrap()
                .target()
                .identity()
                .unwrap()
                .name(),
            &FullName::from_local_name("a.b")
        );
        assert_eq!(
            result
                .context()
                .schema_definition("a.b")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("Quoted Owner")
        );
        Ok(())
    }
}

impl OwnershipIdentity {
    fn qualified_candidate_names(&self, context: &impl ParseContext) -> Vec<FullName> {
        if !Self::owner_schema_scoped(self.kind()) || self.name().schema().is_some() {
            return vec![self.name().clone()];
        }
        let mut names = Vec::new();
        let catalog = SchemaName::from("pg_catalog");
        // PostgreSQL implicitly searches pg_catalog unless it was explicitly placed.
        if !context.get_search_path().contains(&catalog) {
            names.push(self.name().replace_schema(&catalog));
        }
        for schema in context.get_search_path() {
            names.push(self.name().replace_schema(schema));
        }
        names
    }

    fn needs_visible_routine_resolution(&self) -> bool {
        self.arguments().is_none()
            && matches!(
                self.kind(),
                ObjectKind::FUNCTION | ObjectKind::PROCEDURE | ObjectKind::ROUTINE
            )
    }

    fn resolve_qualified_target(&self, context: &impl ParseContext) -> Option<OwnershipLookup> {
        if self.has_incompatible_type_kind(context) {
            return Some(OwnershipLookup::invalid_kind(self));
        }
        let mut identities = self.matching_catalog_identities(context);
        if identities.len() > 1 {
            Some(OwnershipLookup::ambiguous(&identities))
        } else if let Some(identity) = identities.pop() {
            Some(OwnershipLookup::resolved(&identity))
        } else if self.has_incompatible_routine_kind(context) {
            Some(OwnershipLookup::invalid_kind(self))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_alter_type_and_alter_domain_refer_to_the_same_domain_identity {
    use crate::{
        ast::ObjectKind,
        context::TypeInfoRead,
        sem::Role,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[test]
    fn alter_type_and_alter_domain_refer_to_the_same_domain_identity() -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE DOMAIN public.d AS integer")?
                .into_context();
        let result = apply_ownership_sql(context, "ALTER TYPE public.d OWNER TO alice")?;
        let identity = result
            .ast()
            .ownership_change()
            .unwrap()
            .target()
            .identity()
            .unwrap()
            .clone();
        assert_eq!(identity.kind(), ObjectKind::from(13));
        let result =
            apply_ownership_sql(result.into_context(), "ALTER DOMAIN public.d OWNER TO bob")?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().identity(),
            Some(&identity)
        );
        assert_eq!(
            result
                .context()
                .get_type_impl(identity.name())
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        Ok(())
    }
}

impl OwnershipIdentity {
    fn has_incompatible_type_kind(&self, context: &impl ParseContext) -> bool {
        if !matches!(self.kind(), ObjectKind::TYPE | ObjectKind::DOMAIN) {
            return false;
        }
        let Some(definition) = context.get_type_impl(self.name()) else {
            return false;
        };
        match self.kind() {
            ObjectKind::TYPE => {
                definition.is_relation_type() || definition.is_array() == Some(true)
            }
            ObjectKind::DOMAIN => !definition.is_domain(),
            _ => false,
        }
    }

    fn matching_catalog_identities(&self, context: &impl ParseContext) -> Vec<Self> {
        context
            .ownership_definitions(self.kind(), self.name())
            .into_iter()
            .filter(|definition| self.matches(definition.identity()))
            .map(|definition| definition.identity().clone())
            .collect()
    }

    fn has_incompatible_routine_kind(&self, context: &impl ParseContext) -> bool {
        if !matches!(self.kind(), ObjectKind::FUNCTION | ObjectKind::PROCEDURE) {
            return false;
        }
        let requested = if let Some(arguments) = self.arguments() {
            Self::with_arguments(ObjectKind::ROUTINE, self.name(), arguments)
        } else {
            Self::named(ObjectKind::ROUTINE, self.name())
        };
        !requested.matching_catalog_identities(context).is_empty()
    }

    fn resolve_visible_routines(
        &self,
        context: &impl ParseContext,
        names: &[FullName],
    ) -> OwnershipLookup {
        let mut visible = Vec::new();
        for name in names {
            for definition in context.ownership_definitions(ObjectKind::ROUTINE, name) {
                let identity = definition.identity();
                if !Self::contains_signature(&visible, identity) {
                    visible.push(identity.clone());
                }
            }
        }
        let found_other_kind = !visible.is_empty();
        let mut matching: Vec<_> = visible
            .into_iter()
            .filter(|identity| Self::accepts_owner_kind(self.kind(), identity.kind()))
            .collect();
        if matching.len() > 1 {
            OwnershipLookup::ambiguous(&matching)
        } else if let Some(identity) = matching.pop() {
            OwnershipLookup::resolved(&identity)
        } else if found_other_kind {
            OwnershipLookup::invalid_kind(self)
        } else {
            OwnershipLookup::not_found(self)
        }
    }
}

#[cfg(test)]
mod test_omitted_routine_signatures_consider_all_visible_search_path_candidates {
    use crate::{
        ast::ObjectKind,
        context::{OwnershipInfoRead, ParseContext},
        names::{FullName, SchemaName},
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[rstest::rstest]
    #[case::distinct_signatures("text", "ownership_target_ambiguous")]
    #[case::shadowed_signature("integer", "resolved")]
    fn omitted_routine_signatures_consider_all_visible_search_path_candidates(
        #[case] second_type: &str,
        #[case] expected: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION first.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let mut context = apply_ownership_sql(context,
            &format!("CREATE FUNCTION second.f({second_type}) RETURNS {second_type} LANGUAGE sql AS 'SELECT $1'"))?
            .into_context();
        context.set_search_path(&[SchemaName::from("first"), SchemaName::from("second")]);
        let result = apply_ownership_sql(context, "ALTER FUNCTION f OWNER TO alice")?;
        let change = result.ast().ownership_change().unwrap();
        assert_eq!(change.target().code(), expected);
        let second = result
            .context()
            .ownership_definitions(ObjectKind::from(20), &FullName::with_schema("second", "f"));
        assert!(second[0].owner().is_none());
        Ok(())
    }
}

impl OwnershipIdentity {
    fn contains_signature(identities: &[Self], candidate: &Self) -> bool {
        for identity in identities {
            if identity.arguments() == candidate.arguments() {
                return true;
            }
        }
        false
    }
}
