use crate::{
    ast::Dialect,
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, CatalogNameLookup, TypeReference, problem::CollateProblem,
    },
};

/// Completed explicit declaration collation lookup and applicability decision.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeclarationCollation {
    lookup: Option<Box<CatalogNameLookup>>,
    outcome: DeclarationCollationOutcome,
}

#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum DeclarationCollationOutcome {
    #[default]
    Omitted,
    Applicable,
    UnresolvedType,
    Missing,
    Ambiguous,
    NonCollatable,
    UndefinedCapability,
}

impl DeclarationCollation {
    pub(crate) const fn omitted() -> &'static Self {
        static OMITTED: DeclarationCollation = DeclarationCollation {
            lookup: None,
            outcome: DeclarationCollationOutcome::Omitted,
        };
        &OMITTED
    }

    pub(crate) fn validate(
        context: &impl ParseContext,
        ty: Option<&TypeReference>,
        lookup: Option<&CatalogNameLookup>,
    ) -> Self {
        let outcome = if context.dialect() != Dialect::postgres() || lookup.is_none() {
            DeclarationCollationOutcome::Omitted
        } else if ty.is_none() {
            DeclarationCollationOutcome::UnresolvedType
        } else if lookup.is_some_and(CatalogNameLookup::is_missing) {
            DeclarationCollationOutcome::Missing
        } else if lookup.is_some_and(|value| value.ambiguous_candidates().is_some()) {
            DeclarationCollationOutcome::Ambiguous
        } else {
            match ty.expect("resolved type").collation_capability(context) {
                Some(true) => DeclarationCollationOutcome::Applicable,
                Some(false) => DeclarationCollationOutcome::NonCollatable,
                None => DeclarationCollationOutcome::UndefinedCapability,
            }
        };
        Self {
            lookup: lookup.map(|lookup| Box::new(lookup.clone())),
            outcome,
        }
    }

    /// Completed lookup, including missing and ambiguous outcomes.
    pub fn lookup(&self) -> Option<&CatalogNameLookup> {
        self.lookup.as_deref()
    }

    /// Whether this declaration's explicit collation permits catalog application.
    pub const fn can_apply(&self) -> bool {
        matches!(
            self.outcome,
            DeclarationCollationOutcome::Omitted | DeclarationCollationOutcome::Applicable
        )
    }

    /// Whether the catalog leaves applicability indeterminate, rather than false.
    pub const fn is_indeterminate(&self) -> bool {
        matches!(
            self.outcome,
            DeclarationCollationOutcome::UndefinedCapability
        )
    }

    /// Whether failed type resolution prevented applicability validation.
    pub const fn has_unresolved_type(&self) -> bool {
        matches!(self.outcome, DeclarationCollationOutcome::UnresolvedType)
    }

    /// Whether a known capability establishes that the type is non-collatable.
    pub const fn is_non_collatable(&self) -> bool {
        matches!(self.outcome, DeclarationCollationOutcome::NonCollatable)
    }

    pub(crate) fn report(
        &self,
        context: &mut impl ParseContext,
        ty: Option<&TypeReference>,
    ) -> Result<(), AnalysisError> {
        let problem = match &self.outcome {
            DeclarationCollationOutcome::Missing => {
                Some(AnalysisProblem::collation_name_not_found(
                    &self
                        .lookup
                        .as_ref()
                        .expect("explicit lookup")
                        .requested()
                        .to_string(),
                ))
            }
            DeclarationCollationOutcome::Ambiguous => {
                Some(AnalysisProblem::collate(CollateProblem::ambiguous(
                    &self
                        .lookup
                        .as_ref()
                        .expect("explicit lookup")
                        .requested()
                        .to_string(),
                )))
            }
            DeclarationCollationOutcome::NonCollatable => Some(AnalysisProblem::collate(
                CollateProblem::operand_type(ty.expect("resolved type")),
            )),
            DeclarationCollationOutcome::UndefinedCapability => Some(AnalysisProblem::collate(
                CollateProblem::undefined_capability(ty.expect("resolved type")),
            )),
            _ => None,
        };
        if let Some(problem) = problem {
            context.report_analysis_problem(problem)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{ast::Dialect, sem::PgBuiltInType};

    use crate::{
        context::{BaseParseContext, TypeInfoRead},
        names::{FullName, TableName},
        sem::{
            CatalogNameLookup, TypeDefinition, TypeReference,
            declaration_collation::DeclarationCollation,
        },
        test_helpers::{
            TestParseContext, collate, declaration_collation::context,
            ownership::apply_ownership_sql,
        },
    };

    #[rstest::rstest]
    #[case::collatable(Some(true), None, true)]
    #[case::non_collatable(Some(false), Some("collate_operand_type"), false)]
    #[case::undefined(None, Some("collate_capability_undefined"), false)]
    fn capability_is_not_inferred(
        #[case] fact: Option<bool>,
        #[case] code: Option<&str>,
        #[case] applicable: bool,
        #[values(false, true)] array: bool,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let name = FullName::with_schema("extension schema", "a.type");
        context.set_get_type_impl_result(&name, &TypeDefinition::builtin(&name, fact));
        let ty = TypeReference::concrete_type_ref(&name, array);
        let requested = FullName::with_schema("collation.schema", "Mixed.Name");
        let lookup = CatalogNameLookup::found(&requested, &requested);
        let validation = DeclarationCollation::validate(&context, Some(&ty), Some(&lookup));
        validation.report(&mut context, Some(&ty))?;
        assert_eq!(validation.can_apply(), applicable);
        assert_eq!(validation.is_indeterminate(), fact.is_none());
        assert_eq!(validation.is_non_collatable(), fact == Some(false));
        assert_eq!(validation.lookup(), Some(&lookup));
        assert_eq!(
            context
                .reported_problems()
                .first()
                .map(crate::sem::AnalysisProblem::code),
            code
        );
        let json = ::serde_json::to_string(&validation)?;
        assert_eq!(validation, ::serde_json::from_str(&json)?);
        Ok(())
    }

    #[rstest::rstest]
    #[case::missing(false, Some("collation_not_found"))]
    #[case::ambiguous(true, Some("collation_name_ambiguous"))]
    fn lookup_failure_precedes_capability(
        #[case] ambiguous: bool,
        #[case] code: Option<&str>,
        #[values(false, true)] unresolved_type: bool,
    ) -> testresult::TestResult {
        let mut context = collate::context(Dialect::postgres());
        let name = FullName::from_local_name("missing");
        let lookup = if ambiguous {
            CatalogNameLookup::ambiguous(
                &name,
                &[
                    FullName::with_schema("a", "missing"),
                    FullName::with_schema("b", "missing"),
                ],
            )
        } else {
            CatalogNameLookup::missing(&name)
        };
        let ty = if unresolved_type {
            None
        } else {
            Some(PgBuiltInType::int4())
        };
        let validation = DeclarationCollation::validate(&context, ty.as_ref(), Some(&lookup));
        validation.report(&mut context, ty.as_ref())?;
        assert!(!validation.can_apply());
        assert_eq!(validation.has_unresolved_type(), unresolved_type);
        assert!(!validation.is_indeterminate());
        assert!(!validation.is_non_collatable());
        assert_eq!(validation.lookup(), Some(&lookup));
        assert_eq!(
            context
                .reported_problems()
                .first()
                .map(crate::sem::AnalysisProblem::code),
            if unresolved_type { None } else { code }
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::table(r#"CREATE TABLE public.rejected(value public."custom.type" COLLATE "C")"#)]
    #[case::composite(r#"CREATE TYPE public.rejected AS (value public."custom.type" COLLATE "C")"#)]
    #[case::domain(r#"CREATE DOMAIN public.rejected AS public."custom.type" COLLATE "C""#)]
    fn declarations_gate_catalog_publication(
        #[case] sql: &str,
        #[values(Some(true), Some(false), None)] fact: Option<bool>,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(context(fact), sql)?;
        let name = FullName::with_schema("public", "rejected");
        assert_eq!(
            pair.context().get_type_impl(&name).is_some(),
            fact == Some(true)
        );
        assert_eq!(
            pair.context().problems().len(),
            usize::from(fact != Some(true))
        );
        let validation = pair.ast().first_declaration_collation();
        assert_eq!(validation.can_apply(), fact == Some(true));
        assert_eq!(
            validation.lookup().unwrap().resolved_name(),
            Some(&FullName::with_schema("pg_catalog", "C"))
        );
        let pair = apply_ownership_sql(
            pair.into_context(),
            "CREATE TABLE public.later(value integer)",
        )?;
        assert!(
            pair.context()
                .get_table(&TableName::from(FullName::with_schema("public", "later")))
                .is_some()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::table(r#"CREATE TABLE public.rejected(ok text, value integer COLLATE missing)"#)]
    #[case::composite(r#"CREATE TYPE public.rejected AS (ok text, value integer COLLATE missing)"#)]
    #[case::domain(r#"CREATE DOMAIN public.rejected AS integer COLLATE missing"#)]
    fn missing_collation_leaves_no_definition(#[case] sql: &str) -> testresult::TestResult {
        let pair = apply_ownership_sql(context(Some(true)), sql)?;
        assert!(
            pair.context()
                .get_type_impl(&FullName::with_schema("public", "rejected"))
                .is_none()
        );
        assert_eq!(pair.context().problems().len(), 1);
        assert_eq!(pair.context().problems()[0].code(), "collation_not_found");
        Ok(())
    }

    #[test]
    fn rejected_add_column_is_atomic_and_does_not_enter_unimplemented_application()
    -> testresult::TestResult {
        let pair =
            apply_ownership_sql(context(Some(false)), "CREATE TABLE public.t(value integer)")?;
        let pair = apply_ownership_sql(
            pair.into_context(),
            r#"ALTER TABLE public.t ADD COLUMN ok text, ADD COLUMN bad integer COLLATE "C""#,
        )?;
        assert!(!pair.ast().alter_table_statement().unwrap().can_apply());
        let table = pair
            .context()
            .get_table(&TableName::from(FullName::with_schema("public", "t")))
            .unwrap();
        assert!(table.get_column_at(0).is_some());
        assert!(table.get_column_at(1).is_none());
        assert_eq!(pair.context().problems().len(), 1);
        Ok(())
    }

    #[rstest::rstest]
    #[case::table(r#"CREATE TABLE public.rejected(value integer COLLATE "C")"#)]
    #[case::composite(r#"CREATE TYPE public.rejected AS (value integer COLLATE "C")"#)]
    #[case::domain(r#"CREATE DOMAIN public.rejected AS integer COLLATE "C""#)]
    fn application_uses_retained_outcome_without_diagnostic_collection(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(context(Some(false)), sql)?;
        assert!(!pair.ast().first_declaration_collation().can_apply());
        // A fresh application context contains none of the analysis diagnostics.
        let applied = pair.ast().apply_declaration(context(Some(false)))?;
        assert!(applied.problems().is_empty());
        assert!(
            applied
                .get_type_impl(&FullName::with_schema("public", "rejected"))
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn public_composite_analysis_entry_point_validates_fields() -> testresult::TestResult {
        use crate::{sem::CreateCompositeType, syn::Opt, test_helpers::SynBuilder};
        let syntax = SynBuilder::new()
            .parse(r#"CREATE TYPE public.rejected AS (value integer COLLATE "C")"#);
        let pair = CreateCompositeType::analyze_create_comp_type(
            context(Some(false)),
            &None,
            syntax.as_composite_type_stmt().as_inner().unwrap(),
        )?;
        assert!(
            pair.context()
                .get_type_impl(&FullName::with_schema("public", "rejected"))
                .is_none()
        );
        assert_eq!(pair.context().problems()[0].code(), "collate_operand_type");
        Ok(())
    }

    #[rstest::rstest]
    #[case::table(
        r#"CREATE TABLE public.accepted(value varchar(12) COLLATE "collation.schema"."Mixed.Name")"#
    )]
    #[case::composite(
        r#"CREATE TYPE public.accepted AS (value varchar(12) COLLATE "collation.schema"."Mixed.Name")"#
    )]
    #[case::domain(
        r#"CREATE DOMAIN public.accepted AS varchar(12) COLLATE "collation.schema"."Mixed.Name""#
    )]
    fn modifiers_and_qualified_collation_survive_analysis(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(context(Some(true)), sql)?;
        assert!(
            pair.context().problems().is_empty(),
            "{:?}",
            pair.context().problems()
        );
        assert!(
            pair.context()
                .get_type_impl(&FullName::with_schema("public", "accepted"))
                .is_some()
        );
        let validation = pair.ast().first_declaration_collation();
        assert!(validation.can_apply());
        assert_eq!(
            validation.lookup().unwrap().resolved_name(),
            Some(&FullName::with_schema("collation.schema", "Mixed.Name"))
        );
        let requested = pair.ast().first_requested_type();
        assert_eq!(
            requested.value(),
            &crate::sem::TypeReference::from_master_type_name("varchar")
        );
        assert_eq!(requested.arguments(), &["12"]);
        assert_eq!(requested.typmod(), Some(16));
        Ok(())
    }

    #[rstest::rstest]
    #[case::table(
        "CREATE TABLE public.rejected(value public.absent COLLATE public.missing)",
        "column_type_not_found"
    )]
    #[case::composite(
        "CREATE TYPE public.rejected AS (value public.absent COLLATE public.missing)",
        "column_type_not_found"
    )]
    #[case::domain(
        "CREATE DOMAIN public.rejected AS public.absent COLLATE public.missing",
        "domain_base_type_not_found"
    )]
    fn missing_type_is_primary_and_rejected_ddl_remains_inspectable(
        #[case] sql: &str,
        #[case] code: &str,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(context(Some(true)), sql)?;
        assert_eq!(pair.context().problems().len(), 1);
        assert_eq!(pair.context().problems()[0].code(), code);
        let validation = pair.ast().first_declaration_collation();
        assert!(!validation.can_apply());
        assert!(validation.lookup().unwrap().is_missing());
        assert!(
            pair.context()
                .get_type_impl(&FullName::with_schema("public", "rejected"))
                .is_none()
        );
        Ok(())
    }
}
