use crate::sem::problem::CollateProblem;
use crate::{
    ast::Dialect,
    context::ParseContext,
    sem::{AnalysisError, AnalysisProblem, CatalogNameLookup, TypeReference},
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
    use super::DeclarationCollation;
    use crate::{
        sem::{CatalogNameLookup, FullName, TypeDefinition, TypeReference},
        test_helpers::{TestParseContext, collate},
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
        let mut context = collate::context(crate::ast::Dialect::postgres());
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
            Some(crate::sem::PgBuiltInType::int4())
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
}

#[cfg(test)]
mod schema_tests;
