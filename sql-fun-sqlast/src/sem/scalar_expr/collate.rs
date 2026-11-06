use crate::sem::CommonType;
use crate::{
    StringSpan,
    ast::Dialect,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, CatalogNameLookup, ParseContext, SemScalarExpr,
        TypeReference,
        problem::CollateProblem,
        scalar_expr::SemScalarExprNode,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// An explicit COLLATE annotation with its completed name lookup and operand.
/// Expression-wide collation derivation is a separate semantic analysis.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollateExpr {
    argument: Box<SemScalarExpr>,
    collation: CatalogNameLookup,
    source_span: Option<StringSpan>,
    checked_type: Option<TypeReference>,
}

impl CollateExpr {
    fn new(
        argument: &SemScalarExpr,
        collation: &CatalogNameLookup,
        source_span: &Option<StringSpan>,
    ) -> Self {
        Self {
            argument: Box::new(argument.clone()),
            collation: collation.clone(),
            source_span: *source_span,
            checked_type: None,
        }
    }

    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        argument: &SemScalarExpr,
        name: &FullName,
        source_span: &Option<StringSpan>,
    ) -> Result<Self, AnalysisError> {
        let collation = if context.dialect() == Dialect::sqlite() {
            if let Some(resolved) = context.resolve_sqlite_collation(name) {
                CatalogNameLookup::found(name, &resolved)
            } else {
                CatalogNameLookup::missing(name)
            }
        } else {
            context.lookup_collation(name)
        };
        if collation.is_missing() {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(
                &name.to_string(),
            ))?;
        } else if collation.ambiguous_candidates().is_some() {
            context.report_analysis_problem(AnalysisProblem::collate(
                CollateProblem::ambiguous(&name.to_string()),
            ))?;
        }
        let mut value = Self::new(argument, &collation, source_span);
        value.validate_type(context)?;
        Ok(value)
    }
}

#[cfg(test)]
mod test_collate_resolution {
    use crate::{
        ast::Dialect,
        names::FullName,
        sem::{CatalogNameLookup, ParseContext, PgBuiltInType, SemScalarExpr},
        test_helpers::collate::{context, expression, query},
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::local(r#"SELECT 'x' COLLATE "C""#, FullName::from_local_name("C"))]
    #[case::qualified(
        r#"SELECT 'x' COLLATE pg_catalog."C""#,
        FullName::with_schema("pg_catalog", "C")
    )]
    fn retains_completed_postgres_binding(
        #[case] sql: &str,
        #[case] requested: FullName,
    ) -> testresult::TestResult {
        let mut context = context(Dialect::postgres());
        let query = query(sql, &mut context)?;
        let value = expression(&query);
        let collate = value.as_collate().unwrap();
        assert_eq!(collate.collation().requested(), &requested);
        assert_eq!(
            collate.collation().resolved_name(),
            Some(&FullName::with_schema("pg_catalog", "C"))
        );
        assert!(collate.argument().as_const().is_some());
        assert!(collate.source_span().is_some());
        let encoded = ::serde_json::to_string(value)?;
        let restored: SemScalarExpr = ::serde_json::from_str(&encoded)?;
        assert_eq!(value, &restored);
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }

    #[test]
    fn grouping_copy_uses_identity_and_preserves_original_spelling() -> testresult::TestResult {
        let mut context = context(Dialect::postgres());
        let local = query(r#"SELECT 'x'::text COLLATE "C""#, &mut context)?;
        let qualified = query(r#"SELECT 'x'::text COLLATE pg_catalog."C""#, &mut context)?;
        let original = expression(&local).as_collate().unwrap();
        let mut local_key = original.clone();
        let mut qualified_key = expression(&qualified).as_collate().unwrap().clone();
        local_key.canonicalize_grouping_key();
        qualified_key.canonicalize_grouping_key();
        assert_eq!(local_key, qualified_key);
        assert_eq!(
            original.collation().requested(),
            &FullName::from_local_name("C")
        );
        assert!(original.source_span().is_some());
        assert!(original.checked_type.is_some());
        assert!(local_key.source_span().is_none());
        assert!(local_key.checked_type.is_none());
        Ok(())
    }

    #[test]
    fn retains_missing_inner_binding_and_successful_outer_binding() -> testresult::TestResult {
        let mut context = context(Dialect::postgres());
        let query = query(
            r#"SELECT ('x' COLLATE absent484) COLLATE "C""#,
            &mut context,
        )?;
        let outer = expression(&query).as_collate().unwrap();
        let inner = outer.argument().as_collate().unwrap();
        assert!(inner.collation().is_missing());
        assert_eq!(inner.collation().requested().to_string(), "absent484");
        assert!(outer.collation().resolved_name().is_some());
        assert!(context.has_reported_problem("collation_not_found"));
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }

    #[test]
    fn retains_ambiguous_lookup() -> testresult::TestResult {
        let mut context = context(Dialect::postgres());
        let name = FullName::from_local_name("conflicting");
        let candidates = [
            FullName::with_schema("one", "conflicting"),
            FullName::with_schema("two", "conflicting"),
        ];
        context.set_collation_lookup(&name, &CatalogNameLookup::ambiguous(&name, &candidates));
        let query = query("SELECT 'x' COLLATE conflicting", &mut context)?;
        assert_eq!(
            expression(&query)
                .as_collate()
                .unwrap()
                .collation()
                .ambiguous_candidates(),
            Some(candidates.as_slice())
        );
        assert!(context.has_reported_problem("collation_name_ambiguous"));
        Ok(())
    }

    #[rstest::rstest]
    #[case::integer(r#"SELECT 1 COLLATE "C""#, true)]
    #[case::null(r#"SELECT NULL COLLATE "C""#, false)]
    #[case::text(r#"SELECT 'x'::text COLLATE "C""#, false)]
    #[case::unknown(r#"SELECT 'x' COLLATE "C""#, false)]
    #[case::deferred(r#"SELECT $1 COLLATE "C", $1::integer"#, true)]
    fn checks_applicability_when_type_is_known(
        #[case] sql: &str,
        #[case] invalid: bool,
    ) -> testresult::TestResult {
        let mut context = context(Dialect::postgres());
        let query = query(sql, &mut context)?;
        assert!(expression(&query).as_collate().is_some());
        assert_eq!(
            context.has_reported_problem("collate_operand_type"),
            invalid
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::direct(r#"SELECT $1 COLLATE "C", $1::text"#)]
    #[case::nested(r#"SELECT ($1 COLLATE "C") COLLATE "POSIX", $1::text"#)]
    #[case::consumed(r#"SELECT ($1 COLLATE "C")::text"#)]
    fn operand_constraints_survive_annotations(#[case] sql: &str) -> testresult::TestResult {
        let mut context = context(Dialect::postgres());
        let query = query(sql, &mut context)?;
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::text()));
        assert!(
            context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::text())]))
        );
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::builtin("SELECT 'x' COLLATE NOCASE")]
    #[case::custom("SELECT 'x' COLLATE absent484")]
    #[case::integer("SELECT 1 COLLATE BINARY")]
    #[case::null("SELECT NULL COLLATE RTRIM")]
    fn sqlite_missing_names_are_static_diagnostics(#[case] sql: &str) -> testresult::TestResult {
        let mut context = context(Dialect::sqlite());
        let query = query(sql, &mut context)?;
        let value = expression(&query).as_collate().unwrap();
        assert!(value.collation().is_missing());
        assert!(value.source_span().is_none());
        assert!(context.has_reported_problem("collation_not_found"));
        assert!(!context.has_reported_problem("collate_operand_type"));
        // SQLite can execute these projections without consulting a comparator.
        // Static resolution intentionally diagnoses against loaded metadata instead.
        let connection = ::rusqlite::Connection::open_in_memory()?;
        connection.prepare(sql)?.query([])?.next()?.unwrap();
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_resolves_every_nested_reference_and_continues() -> testresult::TestResult {
        let mut context = context(Dialect::sqlite());
        let first = query("SELECT ('x' COLLATE NOCASE) COLLATE BINARY", &mut context)?;
        let outer = expression(&first).as_collate().unwrap();
        assert!(outer.collation().is_missing());
        assert!(
            outer
                .argument()
                .as_collate()
                .unwrap()
                .collation()
                .is_missing()
        );
        assert_eq!(context.reported_problem_count(), 2);
        let next = query("SELECT 1", &mut context)?;
        assert_eq!(expression(&next).get_type(), Some(PgBuiltInType::int4()));
        Ok(())
    }
}

impl CollateExpr {
    /// Analyzed operand, including nested explicit annotations and failed bindings.
    pub const fn argument(&self) -> &SemScalarExpr {
        &self.argument
    }

    /// Completed collation lookup, retaining requested spelling and failure state.
    pub const fn collation(&self) -> &CatalogNameLookup {
        &self.collation
    }

    /// Parser-provided location of the COLLATE annotation, when available.
    pub const fn source_span(&self) -> Option<&StringSpan> {
        self.source_span.as_ref()
    }

    pub(crate) fn validate_type(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(());
        }
        let Some(ty) = SemScalarExpr::common_type_input(&self.argument) else {
            return Ok(());
        };
        // Unknown literals are legal here; resolving their type is the owning
        // expression's job. Missing catalog facts do not prove inapplicability.
        if ty == CommonType::unknown() || self.checked_type.as_ref() == Some(&ty) {
            return Ok(());
        }
        if let Some(collatable) = ty.collation_capability(context) {
            if !collatable {
                context.report_analysis_problem(AnalysisProblem::collate(
                    CollateProblem::operand_type(&ty),
                ))?;
            }
            self.checked_type = Some(ty);
        }
        Ok(())
    }

    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.source_span = None;
        self.checked_type = None;
        if let Some(resolved) = self.collation.resolved_name() {
            self.collation = CatalogNameLookup::found(resolved, resolved);
        }
    }

    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.argument.walk(visitor)
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.argument.visit(visitor)
    }
}

impl SemScalarExprNode for CollateExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.argument.get_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        self.argument.is_not_null()
    }
}

#[cfg(test)]
mod test_collate_operand_metadata {
    use crate::{
        ast::Dialect,
        test_helpers::collate::{context, expression, query},
    };

    #[rstest::rstest]
    #[case::literal(r#"SELECT 'x' COLLATE "C""#)]
    #[case::null(r#"SELECT NULL COLLATE "C""#)]
    #[case::typed_null(r#"SELECT NULL::text COLLATE "C""#)]
    #[case::missing_operand(r#"SELECT absent484 COLLATE "C""#)]
    fn preserves_operand_type_and_nullability(#[case] sql: &str) -> testresult::TestResult {
        let mut context = context(Dialect::postgres());
        let query = query(sql, &mut context)?;
        let value = expression(&query);
        let argument = value.as_collate().unwrap().argument();
        assert_eq!(value.get_type(), argument.get_type());
        assert_eq!(value.is_not_null(), argument.is_not_null());
        Ok(())
    }
}
