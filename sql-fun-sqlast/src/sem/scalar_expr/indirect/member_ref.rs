use crate::sem::scalar_expr::indirect::IndirectionExpr;
use crate::sem::{AnalysisError, ParseContext, SemScalarExpr, TypeReference};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemberRef {
    /// composite type
    base: SemScalarExpr,
    /// accessing member
    member: String,
    /// member type
    member_type: Option<TypeReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    member_not_null: Option<bool>,
}

impl MemberRef {
    pub(super) fn rename_column(&mut self, rename: &crate::sem::ColumnRename) {
        if self.base.get_type().is_some_and(|ty| {
            ty == TypeReference::concrete_type_ref(rename.table().full_name(), false)
        }) && self
            .base
            .as_whole_row()
            .is_none_or(|row| row.has_renamed_field(rename))
        {
            let mut name = crate::sem::ColumnName::from(self.member.as_str());
            rename.rename_name(&mut name);
            self.member = name.as_str().to_owned();
        }
    }

    pub const fn new(
        base: SemScalarExpr,
        member: String,
        member_type: Option<TypeReference>,
        member_not_null: Option<bool>,
    ) -> Self {
        Self {
            base,
            member,
            member_type,
            member_not_null,
        }
    }
    pub fn base_is_not_null(&self) -> Option<bool> {
        match (self.base.is_not_null(), self.member_not_null) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        }
    }

    pub(super) fn row_projection_name(&self) -> &str {
        if self.base.row_constructor().is_some()
            || self.base.constructor_projection_name() == Some("row")
        {
            &self.member
        } else {
            ""
        }
    }

    /// Base expression whose composite field is accessed.
    pub const fn base(&self) -> &SemScalarExpr {
        &self.base
    }

    /// Field identifier after SQL identifier normalization.
    pub fn member(&self) -> &str {
        &self.member
    }

    pub const fn member_type(&self) -> &Option<TypeReference> {
        &self.member_type
    }

    fn test_member_exist<TParseContext>(
        member: &str,
        context: &mut TParseContext,
        source: &SemScalarExpr,
        ty: &TypeReference,
    ) -> Result<(Option<TypeReference>, Option<bool>), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        if let Some(columns) = source.composite_columns(context)
            && let Some(field) = columns.get_column_def(&crate::sem::ColumnName::from(member))
        {
            Ok((field.get_type_cloned(), field.is_not_null()))
        } else {
            if let Some(problem) = ty.member_access_problem(context, member) {
                context.report_analysis_problem(problem)?;
            }
            Ok((None, None))
        }
    }

    pub fn member_reference<TParseContext>(
        source: SemScalarExpr,
        member: String,
        context: &mut TParseContext,
    ) -> Result<IndirectionExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::member_resolved(source, member, context)
    }

    pub(super) fn member_resolved<TContext: crate::sem::ParseContext>(
        source: SemScalarExpr,
        member: String,
        context: &mut TContext,
    ) -> Result<IndirectionExpr, AnalysisError> {
        let (member_type, member_not_null) = if let Some(ty) = &source.get_type() {
            Self::test_member_exist(&member, context, &source, ty)?
        } else {
            (None, None)
        };
        Ok(IndirectionExpr::MemberRef(Box::new(Self::new(
            source,
            member,
            member_type,
            member_not_null,
        ))))
    }
}

#[cfg(test)]
mod domain_member_tests {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::{AnalysisProblem, FullName, IndirectionExpr, PgBuiltInType, TypeReference},
        test_helpers::composite_domains,
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::known("SELECT ($1::outer_d).item", "leaf_dd")]
    #[case::deferred("SELECT ($1).item, $1::outer_d", "leaf_dd")]
    #[case::nested("SELECT (($1).item).n, $1::outer_d", "scalar_d")]
    fn domain_members_retain_analyzed_operands_and_constraints(
        #[case] sql: &str,
        #[case] field_type: &str,
        #[values(false, true)] frozen: bool,
    ) {
        let mut cache = crate::test_helpers::create_cache_context();
        let analyzed = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(composite_domains::context(frozen).unwrap(), &mut cache)
            .unwrap();
        assert!(
            analyzed.context().problems().is_empty(),
            "{:?}",
            analyzed.context().problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let expression = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap();
        assert_eq!(
            expression.get_type(),
            Some(composite_domains::reference(field_type))
        );
        let IndirectionExpr::MemberRef(member) = expression.as_indirection().unwrap() else {
            panic!("member binding")
        };
        assert_eq!(
            member.base().get_type(),
            Some(composite_domains::reference(if field_type == "leaf_dd" {
                "outer_d"
            } else {
                "leaf_dd"
            }))
        );
        let constraints = analyzed.context().parameter_constraints().clone();
        let source = composite_domains::reference("outer_d");
        let owned =
            TypeReference::dynamic(source.definition_from_catalog(analyzed.context()).unwrap());
        drop(analyzed);
        assert!(constraints.allows(&BTreeMap::from([(1, owned)])));
        let other =
            composite_domains::owned_domain("other", &composite_domains::reference("outer_row"));
        assert!(!constraints.allows(&BTreeMap::from([(1, other)])));
        assert!(constraints.allows(&BTreeMap::from([(
            1,
            composite_domains::reference("outer_d")
        )])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        assert!(
            constraints
                .result_column(0)
                .unwrap()
                .allows(&composite_domains::reference(field_type))
        );
    }

    #[test]
    fn encountered_domain_assignments_prepare_nested_members_after_analysis() {
        let mut cache = crate::test_helpers::create_cache_context();
        let analyzed = parse_statements("SELECT (($1).item).n, $2::outer_d", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(composite_domains::context(true).unwrap(), &mut cache)
            .unwrap();
        let constraints = analyzed.context().parameter_constraints().clone();
        drop(analyzed);
        assert!(constraints.allows(&BTreeMap::from([(
            1,
            composite_domains::reference("outer_d")
        )])));
        assert!(!constraints.allows(&BTreeMap::from([(
            1,
            composite_domains::reference("leaf_dd")
        )])));
        assert!(!constraints.allows(&BTreeMap::from([(
            1,
            composite_domains::reference("arr_d")
        )])));
    }
    #[test]
    fn previously_unknown_owned_domains_keep_nested_parameter_relationships() {
        let mut cache = crate::test_helpers::create_cache_context();
        let analyzed = parse_statements("SELECT (($1).n).n, ($1).n", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(
                crate::test_helpers::column_rename::schema(&[]).unwrap(),
                &mut cache,
            )
            .unwrap();
        let constraints = analyzed.context().parameter_constraints().clone();
        drop(analyzed);
        let inner = composite_domains::owned_domain(
            "inner",
            &crate::test_helpers::dynamic_types::record(&PgBuiltInType::int4()),
        );
        let outer = composite_domains::owned_domain(
            "outer",
            &crate::test_helpers::dynamic_types::record(&inner),
        );
        assert!(constraints.allows(&BTreeMap::from([(1, outer)])));
        assert!(!constraints.allows(&BTreeMap::from([(1, inner)])));
    }

    #[rstest::rstest]
    #[case::missing(
        "SELECT (NULL::leaf_dd).missing",
        "leaf_dd",
        AnalysisProblem::composite_type_field_not_found(
            &composite_domains::reference("leaf_dd"), "missing"
        )
    )]
    #[case::scalar(
        "SELECT (NULL::scalar_d).n",
        "scalar_d",
        AnalysisProblem::member_access_requires_composite_type(
            &FullName::with_schema("public", "scalar_d"), "n"
        )
    )]
    #[case::array(
        "SELECT (NULL::arr_d).n",
        "arr_d",
        AnalysisProblem::member_access_requires_composite_type(
            &FullName::with_schema("public", "arr_d"), "n"
        )
    )]
    #[case::deferred_missing(
        "SELECT ($1).missing, $1::leaf_dd",
        "leaf_dd",
        AnalysisProblem::composite_type_field_not_found(
            &composite_domains::reference("leaf_dd"), "missing"
        )
    )]
    #[case::deferred_scalar(
        "SELECT ($1).n, $1::scalar_d",
        "scalar_d",
        AnalysisProblem::member_access_requires_composite_type(
            &FullName::with_schema("public", "scalar_d"), "n"
        )
    )]
    fn failed_domain_access_retains_semantics(
        #[case] sql: &str,
        #[case] base: &str,
        #[case] diagnostic: AnalysisProblem,
    ) {
        let mut cache = crate::test_helpers::create_cache_context();
        let analyzed = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .analyze(composite_domains::context(false).unwrap(), &mut cache)
            .unwrap();
        let problems = ::serde_json::to_value(analyzed.context().problems()).unwrap();
        let expected = ::serde_json::to_value(&diagnostic).unwrap();
        assert!(
            problems.as_array().unwrap().contains(&expected),
            "{problems}"
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let expression = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap();
        let IndirectionExpr::MemberRef(member) = expression.as_indirection().unwrap() else {
            panic!("member binding")
        };
        assert!(member.member_type().is_none());
        // Conflicting deferred constraints may leave the parameter type unresolved.
        if !sql.contains("$1") {
            assert_eq!(
                member.base().get_type(),
                Some(composite_domains::reference(base))
            );
        }
    }
}

impl MemberRef {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.base.walk(visitor)?;
        Ok(())
    }
}

impl MemberRef {
    pub(super) fn row_constructor(&self) -> Option<&crate::sem::scalar_expr::RowExpr> {
        self.base
            .row_constructor()?
            .field(&self.member)?
            .row_constructor()
    }

    pub(super) fn set_member_type(&mut self, ty: &Option<TypeReference>) {
        self.member_type.clone_from(ty);
    }

    pub(super) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
    ) -> Option<SemScalarExpr> {
        if let Some(row) = self.base.row_constructor()
            && let Some(field) = row.field(&self.member)
            && field.constraint_node().is_some()
        {
            let mut result = parameters.row_field(field);
            result.retain_indirection(&IndirectionExpr::MemberRef(Box::new(self.clone())));
            return Some(result);
        }
        if self.base.constraint_node().is_none() && !self.base.needs_type_inference() {
            return None;
        }
        let mut result = parameters.member_access(&self.base, &self.member);
        result.retain_indirection(&IndirectionExpr::MemberRef(Box::new(self.clone())));
        Some(result)
    }
}

impl MemberRef {
    pub(super) fn walk_assignment_target<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.base.walk_assignment_target(visitor)?;
        Ok(())
    }
}
