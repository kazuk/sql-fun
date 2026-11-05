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

    fn contains_signature(identities: &[Self], candidate: &Self) -> bool {
        for identity in identities {
            if identity.arguments() == candidate.arguments() {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::ObjectKind,
        context::{
            BaseParseContext, MutSourcePathWithSpan, OwnershipInfoRead, ParseContext, RoleInfoRead,
            TableInfoRead, TypeInfoRead,
        },
        names::{FullName, SchemaName, TableName},
        sem::{
            AlterOwner, OwnedObjectDefinition, OwnerSpecification, OwnershipIdentity,
            PgBuiltInType as SemPgBuiltInType, Role, SemAst, TypeReference,
        },
        syn::Opt,
        test_helpers::{
            SynBuilder, TestParseContext, create_cache_context,
            ownership::{apply_ownership_sql, ownership_context, ownership_statement},
            schema_layer, source_span,
        },
    };

    #[rstest::rstest]
    #[case::aggregate(2, "AGGREGATE public.total(integer)", "total", true, None)]
    #[case::collation(8, "COLLATION public.c", "c", false, None)]
    #[case::conversion(9, "CONVERSION public.c", "c", false, None)]
    #[case::database(10, "DATABASE db", "db", false, None)]
    #[case::event_trigger(15, "EVENT TRIGGER trigger1", "trigger1", false, None)]
    #[case::fdw(17, "FOREIGN DATA WRAPPER wrapper", "wrapper", false, None)]
    #[case::server(18, "SERVER foreign1", "foreign1", false, None)]
    #[case::language(22, "LANGUAGE lang", "lang", false, None)]
    #[case::large_object(23, "LARGE OBJECT 12345", "12345", false, None)]
    #[case::operator_class(
        25,
        "OPERATOR CLASS public.opclass USING btree",
        "opclass",
        false,
        Some("btree")
    )]
    #[case::operator_family(
        27,
        "OPERATOR FAMILY public.opfamily USING hash",
        "opfamily",
        false,
        Some("hash")
    )]
    #[case::publication(31, "PUBLICATION pub1", "pub1", false, None)]
    #[case::statistics(40, "STATISTICS public.stats", "stats", false, None)]
    #[case::subscription(39, "SUBSCRIPTION sub1", "sub1", false, None)]
    #[case::tablespace(43, "TABLESPACE space1", "space1", false, None)]
    #[case::text_configuration(46, "TEXT SEARCH CONFIGURATION public.cfg", "cfg", false, None)]
    #[case::text_dictionary(47, "TEXT SEARCH DICTIONARY public.dict", "dict", false, None)]
    fn resolves_and_applies_catalog_object_classes(
        #[case] kind: u32,
        #[case] target: &str,
        #[case] local: &str,
        #[case] signed: bool,
        #[case] method: Option<&str>,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        let kind = ObjectKind::from(kind);
        let name = if OwnershipIdentity::owner_schema_scoped(kind) {
            FullName::with_schema("public", local)
        } else {
            FullName::from_local_name(local)
        };
        let identity = if signed {
            OwnershipIdentity::with_arguments(kind, &name, &[Some(SemPgBuiltInType::int4())])
        } else if let Some(method) = method {
            OwnershipIdentity::with_access_method(kind, &name, method)
        } else {
            OwnershipIdentity::named(kind, &name)
        };
        context.insert_owned_object(&OwnedObjectDefinition::new(&identity, None));
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO alice"))?;
        let change = result
            .ast()
            .ownership_change()
            .expect("ownership semantics");
        assert_eq!(change.target().identity(), Some(&identity));
        assert_eq!(
            change.owner().resolved_role().map(Role::name),
            Some("alice")
        );
        let definitions = result.context().ownership_definitions(kind, &name);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].owner(), Some(change.owner()));
        let frozen = result.into_context().freeze();
        let next = schema_layer(std::path::Path::new("next.sql"), &frozen);
        assert_eq!(next.ownership_definitions(kind, &name), definitions);
        Ok(())
    }

    #[rstest::rstest]
    #[case::domain("CREATE DOMAIN public.d AS integer", "DOMAIN public.d", 13, "d")]
    #[case::enum_type("CREATE TYPE public.e AS ENUM ('x')", "TYPE public.e", 50, "e")]
    #[case::composite_type("CREATE TYPE public.c AS (value integer)", "TYPE public.c", 50, "c")]
    #[case::schema("CREATE SCHEMA app", "SCHEMA app", 37, "app")]
    #[case::function(
        "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        "FUNCTION public.f(integer)",
        20,
        "f"
    )]
    #[case::procedure(
        "CREATE PROCEDURE public.p(INOUT value integer) LANGUAGE sql AS 'SELECT value'",
        "PROCEDURE public.p(integer)",
        30,
        "p"
    )]
    #[case::routine(
        "CREATE FUNCTION public.r() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "ROUTINE public.r()",
        20,
        "r"
    )]
    fn changes_retained_ddl_models(
        #[case] create: &str,
        #[case] target: &str,
        #[case] kind: u32,
        #[case] local: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO bob"))?;
        assert!(
            result.context().problems().is_empty(),
            "{:?}",
            result.context().problems()
        );
        let kind = ObjectKind::from(kind);
        let name = if OwnershipIdentity::owner_schema_scoped(kind) {
            FullName::with_schema("public", local)
        } else {
            FullName::from_local_name(local)
        };
        let definitions = result.context().ownership_definitions(kind, &name);
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        if kind.code() == 50 || kind.code() == 13 {
            assert_eq!(
                result.context().get_type_impl(&name).unwrap().owner(),
                definitions[0].owner()
            );
        }
        if kind.code() == 20 || kind.code() == 30 {
            assert_eq!(
                result.context().routine_definitions(&name)[0].owner(),
                definitions[0].owner()
            );
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::table("CREATE TABLE public.t (id integer)", "TABLE", 42)]
    #[case::view("CREATE VIEW public.t AS SELECT 1 AS id", "VIEW", 52)]
    #[case::sequence("CREATE SEQUENCE public.t", "SEQUENCE", 38)]
    #[case::materialized(
        "CREATE MATERIALIZED VIEW public.t AS SELECT 1 AS id",
        "MATERIALIZED VIEW",
        24
    )]
    fn relation_commands_keep_their_own_semantic_path(
        #[case] create: &str,
        #[case] target: &str,
        #[case] kind: u32,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(
            context,
            &format!("ALTER {target} public.t OWNER TO CURRENT_USER"),
        )?;
        assert!(result.ast().ownership_change().is_none());
        assert!(
            result.context().problems().is_empty(),
            "{:?}",
            result.context().problems()
        );
        let definitions = result.context().ownership_definitions(
            ObjectKind::from(kind),
            &FullName::with_schema("public", "t"),
        );
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0].owner(),
            Some(&OwnerSpecification::current_user())
        );
        Ok(())
    }

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

    #[test]
    fn missing_roles_and_targets_are_explicit_and_never_apply() -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), "CREATE SCHEMA app")?.into_context();
        let result = apply_ownership_sql(context, "ALTER SCHEMA app OWNER TO absent")?;
        let alteration = result.ast().ownership_change().unwrap();
        assert!(alteration.target().identity().is_some());
        assert_eq!(alteration.owner().name(), Some("absent"));
        assert!(alteration.owner().resolved_role().is_none());
        assert!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .is_none()
        );
        assert_eq!(
            result.context().problems()[0].code(),
            "ownership_role_not_found"
        );
        assert_eq!(
            result.context().problems()[0]
                .ownership_problem()
                .unwrap()
                .role_name(),
            Some("absent")
        );
        let result = apply_ownership_sql(
            result.into_context(),
            "ALTER SCHEMA missing OWNER TO another_absent",
        )?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().code(),
            "ownership_target_not_found"
        );
        assert_eq!(result.context().problems().len(), 3);
        let result = apply_ownership_sql(result.into_context(), "ALTER SCHEMA app OWNER TO alice")?;
        assert_eq!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        Ok(())
    }

    #[test]
    fn resolution_does_not_apply_and_legacy_entry_point_shares_the_result() -> testresult::TestResult
    {
        let mut context =
            apply_ownership_sql(ownership_context(), "CREATE SCHEMA app")?.into_context();
        let statement = ownership_statement("ALTER SCHEMA app OWNER TO alice")?;
        let semantics =
            SemAst::from_ast_node(&statement, &mut context, &mut create_cache_context())?;
        assert!(
            semantics
                .ownership_change()
                .unwrap()
                .target()
                .identity()
                .is_some()
        );
        assert!(context.schema_definition("app").unwrap().owner().is_none());
        let syntax = SynBuilder::new()
            .parse("ALTER SCHEMA app OWNER TO alice")
            .as_alter_owner_stmt()
            .as_inner()
            .unwrap();
        let result = AlterOwner::analyze_alter_owner(context, syntax)?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target(),
            semantics.ownership_change().unwrap().target()
        );
        assert_eq!(
            result.context().schema_definition("app").unwrap().owner(),
            Some(semantics.ownership_change().unwrap().owner())
        );
        Ok(())
    }

    #[test]
    fn exact_overload_changes_survive_freezing_without_changing_base_or_siblings()
    -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION public.f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let base = context.freeze();
        let next = schema_layer(std::path::Path::new("next.sql"), &base);
        let result = apply_ownership_sql(next, "ALTER FUNCTION public.f(integer) OWNER TO alice")?;
        let name = FullName::with_schema("public", "f");
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(20), &name);
        assert_eq!(definitions.len(), 2);
        assert_eq!(
            crate::test_helpers::ownership::known_owner_count(&definitions),
            1
        );
        assert_eq!(
            crate::test_helpers::ownership::known_owner_count(
                &base.ownership_definitions(ObjectKind::from(20), &name)
            ),
            0
        );
        assert_eq!(
            result.context().get_function_by_name(&name).unwrap().len(),
            2
        );
        let frozen = result.into_context().freeze();
        assert_eq!(
            frozen.ownership_definitions(ObjectKind::from(20), &name),
            definitions
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::ambiguous("ALTER FUNCTION public.f OWNER TO alice", "ownership_target_ambiguous")]
    #[case::no_implicit_cast(
        "ALTER FUNCTION public.f(bigint) OWNER TO alice",
        "ownership_target_not_found"
    )]
    #[case::missing_type(
        "ALTER FUNCTION public.f(absent) OWNER TO alice",
        "ownership_argument_type_not_found"
    )]
    #[case::wrong_kind(
        "ALTER PROCEDURE public.f(integer) OWNER TO alice",
        "ownership_invalid_target_kind"
    )]
    #[case::empty_signature(
        "ALTER FUNCTION public.f() OWNER TO alice",
        "ownership_target_not_found"
    )]
    fn invalid_overload_lookups_preserve_catalog(
        #[case] sql: &str,
        #[case] expected: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION public.f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let result = apply_ownership_sql(context, sql)?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().code(),
            expected
        );
        assert_eq!(result.context().problems()[0].code(), expected);
        assert_eq!(
            crate::test_helpers::ownership::known_owner_count(
                &result.context().ownership_definitions(
                    ObjectKind::from(20),
                    &FullName::with_schema("public", "f")
                )
            ),
            0
        );
        Ok(())
    }

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

    #[rstest::rstest]
    #[case::binary("OPERATOR public.+ (integer, integer)", &[Some(crate::sem::PgBuiltInType::int4()), Some(crate::sem::PgBuiltInType::int4())])]
    #[case::unary("OPERATOR public.! (NONE, integer)", &[None, Some(crate::sem::PgBuiltInType::int4())])]
    fn operator_identity_includes_none_operands(
        #[case] target: &str,
        #[case] arguments: &[Option<TypeReference>],
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        let symbol = if arguments[0].is_none() { "!" } else { "+" };
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::from(26),
            &FullName::with_schema("public", symbol),
            arguments,
        );
        context.insert_owned_object(&OwnedObjectDefinition::new(&identity, None));
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO bob"))?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().identity(),
            Some(&identity)
        );
        assert_eq!(
            result
                .context()
                .ownership_definitions(ObjectKind::from(26), identity.name())[0]
                .owner(),
            Some(result.ast().ownership_change().unwrap().owner())
        );
        Ok(())
    }

    #[test]
    fn table_row_type_reads_follow_the_relation_owner_and_cannot_be_altered_independently()
    -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO alice")?.into_context();
        let name = FullName::with_schema("public", "t");
        let definitions = context.ownership_definitions(ObjectKind::from(50), &name);
        assert_eq!(
            definitions[0].owner(),
            context.get_table_impl(&name).unwrap().owner()
        );
        let result = apply_ownership_sql(context, "ALTER TYPE public.t OWNER TO bob")?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().code(),
            "ownership_invalid_target_kind"
        );
        assert_eq!(
            result
                .context()
                .get_table(&TableName::from(name))
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        Ok(())
    }

    #[test]
    fn roles_are_retained_in_the_selected_catalog() {
        let context = ownership_context();
        assert_eq!(context.role_by_name("alice").map(Role::name), Some("alice"));
        assert!(context.role_by_name("ALICE").is_none());
        let frozen = context.freeze();
        assert_eq!(frozen.role_by_name("alice").map(Role::name), Some("alice"));
    }

    #[test]
    fn dependent_sequence_owners_follow_table_changes_across_layers() -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "CREATE SEQUENCE public.s OWNED BY public.t.id")?
                .into_context();
        let base = context.freeze();
        let mut context = schema_layer(std::path::Path::new("next.sql"), &base);
        MutSourcePathWithSpan::set_source_path_with_span(&mut context, &source_span());
        let result = apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO bob")?;
        let name = FullName::with_schema("public", "s");
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(38), &name);
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        assert!(
            base.ownership_definitions(ObjectKind::from(38), &name)[0]
                .owner()
                .is_none()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::out_argument(
        "CREATE FUNCTION public.f(IN a integer, OUT b integer) LANGUAGE sql AS 'SELECT a'",
        "FUNCTION public.f(integer)"
    )]
    #[case::named_signature(
        "CREATE FUNCTION public.f(a integer) RETURNS integer LANGUAGE sql AS 'SELECT a'",
        "FUNCTION public.f(IN a integer)"
    )]
    #[case::variadic_signature(
        "CREATE FUNCTION public.f(VARIADIC a text[]) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "FUNCTION public.f(VARIADIC text[])"
    )]
    #[case::empty_signature(
        "CREATE FUNCTION public.f() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "FUNCTION public.f()"
    )]
    #[case::unspecified_signature(
        "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        "FUNCTION public.f"
    )]
    fn routine_signatures_use_input_identity(
        #[case] create: &str,
        #[case] target: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO alice"))?;
        assert!(
            result.context().problems().is_empty(),
            "{:?}",
            result.context().problems()
        );
        assert!(
            result
                .ast()
                .ownership_change()
                .unwrap()
                .target()
                .identity()
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn missing_owner_does_not_replace_previously_committed_ownership() -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), "CREATE SCHEMA app")?.into_context();
        let context =
            apply_ownership_sql(context, "ALTER SCHEMA app OWNER TO alice")?.into_context();
        let result = apply_ownership_sql(context, "ALTER SCHEMA app OWNER TO missing")?;
        assert_eq!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        let result = apply_ownership_sql(result.into_context(), "ALTER SCHEMA app OWNER TO bob")?;
        assert_eq!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::table("CREATE TABLE public.t (id integer)", "TABLE", 42)]
    #[case::view("CREATE VIEW public.t AS SELECT 1 AS id", "VIEW", 52)]
    #[case::sequence("CREATE SEQUENCE public.t", "SEQUENCE", 38)]
    fn unqualified_relation_changes_replace_the_matched_qualified_key(
        #[case] create: &str,
        #[case] kind: &str,
        #[case] code: u32,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(context, &format!("ALTER {kind} t OWNER TO alice"))?;
        let definitions = result.context().ownership_definitions(
            ObjectKind::from(code),
            &FullName::with_schema("public", "t"),
        );
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        Ok(())
    }

    #[test]
    fn quoted_function_names_cannot_collide_with_schema_separators() -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.\"a.b\"() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        )?
        .into_context();
        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION \"public.a\".b() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        )?
        .into_context();
        let result = apply_ownership_sql(context, "ALTER FUNCTION public.\"a.b\"() OWNER TO bob")?;
        let changed = result
            .context()
            .routine_definitions(&FullName::with_schema("public", "a.b"));
        let untouched = result
            .context()
            .routine_definitions(&FullName::with_schema("public.a", "b"));
        assert_eq!(changed.len(), 1);
        assert!(changed[0].owner().is_some());
        assert_eq!(untouched.len(), 1);
        assert!(untouched[0].owner().is_none());
        Ok(())
    }

    #[rstest::rstest]
    #[case::domain("CREATE DOMAIN public.d AS integer", "DOMAIN", "d")]
    #[case::composite("CREATE TYPE public.d AS (id integer)", "TYPE", "d")]
    #[case::enum_type("CREATE TYPE public.d AS ENUM ('x')", "TYPE", "d")]
    fn ownership_preserves_shared_type_identity_and_serialization(
        #[case] create: &str,
        #[case] kind: &str,
        #[case] name: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let name = FullName::with_schema("public", name);
        let previous = context.get_type_impl(&name).unwrap().clone();
        let serialized = ::serde_json::to_value(&previous)?;
        let result =
            apply_ownership_sql(context, &format!("ALTER {kind} public.d OWNER TO alice"))?;
        let changed = result.context().get_type_impl(&name).unwrap();
        assert!(changed.owner().is_some());
        assert_eq!(changed, &previous);
        assert_eq!(::serde_json::to_value(changed)?, serialized);
        Ok(())
    }

    #[test]
    fn all_missing_argument_type_names_are_retained() -> testresult::TestResult {
        let result = apply_ownership_sql(
            ownership_context(),
            "ALTER FUNCTION public.f(missing_one, missing_two) OWNER TO alice",
        )?;
        let outcome = result.ast().ownership_change().unwrap().target();
        assert_eq!(
            outcome.unresolved_argument_types(),
            &[
                FullName::from_local_name("missing_one"),
                FullName::from_local_name("missing_two")
            ]
        );
        assert_eq!(
            result.context().problems()[0].code(),
            "ownership_argument_type_not_found"
        );
        Ok(())
    }

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

    #[rstest::rstest]
    #[case::zero_argument("AGGREGATE public.total(*)", &[])]
    #[case::ordered_set("AGGREGATE public.total(integer ORDER BY integer)", &[Some(crate::sem::PgBuiltInType::int4()), Some(crate::sem::PgBuiltInType::int4())])]
    fn aggregate_signatures_identify_the_exact_definition(
        #[case] target: &str,
        #[case] arguments: &[Option<TypeReference>],
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::from(2),
            &FullName::with_schema("public", "total"),
            arguments,
        );
        context.insert_owned_object(&OwnedObjectDefinition::new(&identity, None));
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO alice"))?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().identity(),
            Some(&identity)
        );
        Ok(())
    }

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

    #[test]
    fn removed_sequence_dependency_does_not_leave_stale_owner_cascades() -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "CREATE SEQUENCE public.s OWNED BY public.t.id")?
                .into_context();
        let result = apply_ownership_sql(context, "ALTER SEQUENCE public.s OWNED BY NONE")?;
        let context = result.into_context();
        let result = apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO bob")?;
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(38), &FullName::with_schema("public", "s"));
        assert!(definitions[0].owner().is_none());
        Ok(())
    }

    #[rstest::rstest]
    #[case::missing_role("ALTER TABLE public.t OWNER TO missing")]
    #[case::unrelated_change("ALTER TABLE public.t ALTER COLUMN id SET DEFAULT 1")]
    fn only_a_successful_owner_command_cascades_to_sequences(
        #[case] alteration: &str,
    ) -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO alice")?.into_context();
        let context =
            apply_ownership_sql(context, "CREATE SEQUENCE public.s OWNED BY public.t.id")?
                .into_context();
        let result = apply_ownership_sql(context, alteration)?;
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::from(38), &FullName::with_schema("public", "s"));
        assert!(definitions[0].owner().is_none());
        Ok(())
    }

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

    #[rstest::rstest]
    #[case::unknown_base_owner(false)]
    #[case::known_base_owner(true)]
    fn ownership_only_override_wins_over_immutable_base_definition(
        #[case] known_base_owner: bool,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        if known_base_owner {
            context = apply_ownership_sql(context, "ALTER TYPE pg_catalog.int4 OWNER TO alice")?
                .into_context();
        }
        let name = FullName::with_schema("pg_catalog", "int4");
        let before = context.ownership_definitions(ObjectKind::TYPE, &name);
        let base = context.freeze();
        let child = schema_layer(std::path::Path::new("child.sql"), &base);
        let result = apply_ownership_sql(child, "ALTER TYPE pg_catalog.int4 OWNER TO bob")?;
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::TYPE, &name);
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        assert_eq!(base.ownership_definitions(ObjectKind::TYPE, &name), before);
        let child_base = result.into_context().freeze();
        let grandchild = schema_layer(std::path::Path::new("grandchild.sql"), &child_base);
        assert_eq!(
            grandchild.ownership_definitions(ObjectKind::TYPE, &name),
            definitions
        );
        let changed = apply_ownership_sql(grandchild, "ALTER TYPE pg_catalog.int4 OWNER TO alice")?;
        assert_eq!(
            changed
                .context()
                .ownership_definitions(ObjectKind::TYPE, &name)[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        assert_eq!(
            child_base.ownership_definitions(ObjectKind::TYPE, &name),
            definitions
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::named("alice", Some("alice"), None, 0)]
    #[case::missing("missing", None, None, 1)]
    #[case::current("CURRENT_USER", None, Some("CURRENT_USER"), 0)]
    fn schema_authorization_preserves_valid_owners_and_rejects_missing_roles(
        #[case] authorization: &str,
        #[case] role_name: Option<&str>,
        #[case] keyword: Option<&str>,
        #[case] diagnostic_count: usize,
    ) -> testresult::TestResult {
        let result = apply_ownership_sql(
            ownership_context(),
            &format!("CREATE SCHEMA app AUTHORIZATION {authorization}"),
        )?;
        assert_eq!(result.context().problems().len(), diagnostic_count);
        if diagnostic_count != 0 {
            assert!(result.context().schema_definition("app").is_none());
            assert!(
                result
                    .context()
                    .ownership_definitions(ObjectKind::SCHEMA, &FullName::from_local_name("app"))
                    .is_empty()
            );
            assert_eq!(
                result.context().problems()[0]
                    .ownership_problem()
                    .unwrap()
                    .role_name(),
                Some(authorization)
            );
            return Ok(());
        }
        let schema = result.context().schema_definition("app").unwrap();
        let owner = schema.owner().unwrap();
        assert_eq!(owner.resolved_role().map(Role::name), role_name);
        assert_eq!(owner.session_keyword(), keyword);
        if keyword.is_none() {
            assert_eq!(owner.name(), Some(authorization));
        }
        assert_eq!(result.context().problems().len(), diagnostic_count);
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::SCHEMA, &FullName::from_local_name("app"));
        assert_eq!(definitions[0].owner(), Some(owner));
        Ok(())
    }
}
