use crate::sem::{
    AnalysisError, AnalysisProblem, InputTypeConstraint, MinMaxExpr, ParameterConstraints,
    ParseContext, PgBuiltInType, SemScalarExpr, TypeReference,
    parameter_resolution::ParameterResolution,
    problem::TruthTestProblem,
    scalar_expr::SemScalarExprNode,
    traversal::{SemanticVisitor, SemanticVisitorMut},
};

/// A SQL truth predicate with its analyzed operand and boolean coercion.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TruthTestExpr {
    argument: Box<SemScalarExpr>,
    value: Option<bool>,
    negated: bool,
    types: InputTypeConstraint,
    #[serde(skip)]
    node: Option<usize>,
}

impl TruthTestExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        argument: &SemScalarExpr,
        value: Option<bool>,
        negated: bool,
    ) -> Result<Self, AnalysisError> {
        let mut result = Self {
            argument: Box::new(argument.clone()),
            value,
            negated,
            types: InputTypeConstraint::exact(&PgBuiltInType::bool()),
            node: None,
        };
        if let Some(ty) = MinMaxExpr::input_type(argument) {
            if ty == MinMaxExpr::unknown() {
                if let Some(literal) = argument.as_const()
                    && let Some(text) = literal.as_string()
                    && !Self::valid_boolean_input(text)
                {
                    result.fail(context, TruthTestProblem::invalid_literal(text))?;
                } else {
                    // Normalize unknown literals before graph collection: their
                    // apparent text type is not a typed SQL text operand.
                    result.coerce();
                }
            } else if !Self::accepts_type(context, &ty) {
                result.fail(context, TruthTestProblem::operand_type(&ty))?;
            }
        }
        if argument
            .as_truth_test()
            .is_some_and(|test| test.types.admissible_types() == Some(&[]))
        {
            result.types = InputTypeConstraint::types([]);
        }
        // Parameter-free analyses intentionally skip the inference pass.
        if !context.parameter_inference_needed() && result.types.single_type().is_some() {
            result.coerce();
        }
        Ok(result)
    }
}

#[cfg(test)]
mod test_resolve {
    use crate::test_helpers::truth_test as test_helpers;

    use crate::sem::SemScalarExpr;

    use crate::sem::{CastContext, ParseContext, PgBuiltInType};

    use crate::test_helpers::query::expression;

    use crate::test_helpers::statements::query_postgres as query;

    use crate::test_helpers::truth_test::context;

    use crate::test_helpers::truth_test::schema;

    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::is_true("TRUE", Some(true), false)]
    #[case::is_not_true("NOT TRUE", Some(true), true)]
    #[case::is_false("FALSE", Some(false), false)]
    #[case::is_not_false("NOT FALSE", Some(false), true)]
    #[case::is_unknown("UNKNOWN", None, false)]
    #[case::is_not_unknown("NOT UNKNOWN", None, true)]
    fn retains_all_six_kinds_and_operand_coercions(
        #[case] kind: &str,
        #[case] value: Option<bool>,
        #[case] negated: bool,
        #[values("TRUE", "FALSE", "NULL", "NULL::boolean", "$1")] operand: &str,
    ) -> testresult::TestResult {
        let (query, _) = query(&format!("SELECT {operand} IS {kind}"), context(None))?;
        let expr = expression(&query);
        let test = expr.as_truth_test().unwrap();
        assert_eq!(test.value(), value);
        assert_eq!(test.negated(), negated);
        assert_eq!(test.argument().get_type(), Some(PgBuiltInType::bool()));
        let encoded = ::serde_json::to_string(expr)?;
        let decoded: SemScalarExpr = ::serde_json::from_str(&encoded)?;
        assert_eq!(decoded.as_truth_test().unwrap().value(), value);
        assert_eq!(decoded.as_truth_test().unwrap().negated(), negated);
        Ok(())
    }

    #[rstest::rstest]
    #[case::integer("1 IS TRUE")]
    #[case::typed_text("'true'::text IS TRUE")]
    #[case::invalid("'invalid' IS FALSE")]
    #[case::ambiguous("'o' IS TRUE")]
    #[case::empty("'' IS UNKNOWN")]
    #[case::invalid_nested("(1 IS TRUE) IS FALSE")]
    fn retains_invalid_operands_and_empty_result_domains(
        #[case] input: &str,
        #[values("", ", $1")] parameter: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(&format!("SELECT {input}{parameter}"), context(None))?;
        let test = expression(&query).as_truth_test().unwrap();
        assert_eq!(test.types().admissible_types(), Some([].as_slice()));
        if !parameter.is_empty() {
            assert_eq!(
                context
                    .parameter_constraints()
                    .result_column(0)
                    .unwrap()
                    .admissible_types(),
                Some([].as_slice())
            );
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::identity(None, false)]
    #[case::implicit(Some(CastContext::implicit()), true)]
    #[case::assignment(Some(CastContext::assignment()), false)]
    #[case::explicit(Some(CastContext::explicit()), false)]
    fn accepts_only_implicit_boolean_conversions(
        #[case] cast: Option<CastContext>,
        #[case] accepted: bool,
    ) -> testresult::TestResult {
        let (query, _) = query("SELECT 'true'::text IS TRUE", context(cast))?;
        let test = expression(&query).as_truth_test().unwrap();
        assert_eq!(test.types().single_type().is_some(), accepted);
        if accepted {
            assert_eq!(test.argument().get_type(), Some(PgBuiltInType::bool()));
            let encoded = ::serde_json::to_value(test.argument())?;
            assert!(encoded.get("ImplicitCast").is_some());
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::nullable("nullable", Some(false))]
    #[case::not_null("required", Some(true))]
    fn resolves_columns_and_preserves_operand_nullability(
        #[case] column: &str,
        #[case] nullable: Option<bool>,
        #[values("TRUE", "NOT TRUE", "FALSE", "NOT FALSE", "UNKNOWN", "NOT UNKNOWN")] kind: &str,
    ) -> testresult::TestResult {
        let context = schema("CREATE TABLE flags(nullable boolean, required boolean NOT NULL)")?;
        let (query, _) = query(&format!("SELECT {column} IS {kind} FROM flags"), context)?;
        let expr = expression(&query);
        let argument = expr.as_truth_test().unwrap().argument();
        assert_eq!(argument.is_not_null(), nullable);
        assert_eq!(
            argument.get_column_def().unwrap().get_type_cloned(),
            Some(PgBuiltInType::bool())
        );
        Ok(())
    }

    #[test]
    fn accepts_boolean_domain_operands_and_parameters() -> testresult::TestResult {
        let context = schema("CREATE DOMAIN flag AS boolean; CREATE TABLE flags(value flag)")?;
        let (query, context) = query("SELECT value IS UNKNOWN, $1 IS TRUE FROM flags", context)?;
        assert_eq!(
            expression(&query)
                .as_truth_test()
                .unwrap()
                .argument()
                .get_type(),
            Some(PgBuiltInType::bool())
        );
        assert!(
            context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, test_helpers::bool_domain())]))
        );
        Ok(())
    }

    #[test]
    fn continues_after_invalid_truth_test() -> testresult::TestResult {
        let (_, context) = query("SELECT 1 IS TRUE", context(None))?;
        let (query, _) = query("SELECT false IS FALSE", context)?;
        assert!(expression(&query).as_truth_test().is_some());
        Ok(())
    }

    #[rstest::rstest]
    #[case::missing("SELECT missing IS TRUE FROM flags", "UndefinedColumn")]
    #[case::ambiguous("SELECT value IS TRUE FROM flags a, flags b", "AmbiguousColumnRef")]
    fn retains_failed_column_bindings(
        #[case] sql: &str,
        #[case] binding: &str,
    ) -> testresult::TestResult {
        let context = schema("CREATE TABLE flags(value boolean)")?;
        let (query, _) = query(sql, context)?;
        let encoded =
            ::serde_json::to_value(expression(&query).as_truth_test().unwrap().argument())?;
        // A boolean coercion must retain the completed failed lookup underneath it.
        assert!(
            encoded["ImplicitCast"]["value"]["ColumnRef"]
                .get(binding)
                .is_some(),
            "{encoded}"
        );
        Ok(())
    }
}

impl TruthTestExpr {
    pub(crate) fn accepts_type(context: &impl ParseContext, ty: &TypeReference) -> bool {
        let canonical = if ty.is_dynamic() {
            ty.clone()
        } else {
            context
                .canonicalize_type_ref(ty)
                .unwrap_or_else(|| ty.clone())
        };
        MinMaxExpr::base_type(context, &canonical)
            .implicit_match_context(&PgBuiltInType::bool(), context)
            .is_some_and(|cast| cast.use_in_implicit())
    }

    pub(crate) fn input_types(context: &impl ParseContext) -> InputTypeConstraint {
        let target = PgBuiltInType::bool();
        let mut sources = context.cast_sources(&target, false);
        // As in common-type resolution, domain inputs are checked through
        // their base type, while the admissible parameter identity stays nominal.
        for ty in context.type_references() {
            if Self::accepts_type(context, &ty) {
                if !ty.is_dynamic()
                    && let Some(canonical) = context.canonicalize_type_ref(&ty)
                {
                    sources.insert(canonical);
                }
                sources.insert(ty);
            }
        }
        InputTypeConstraint::types(sources)
    }

    fn fail(
        &mut self,
        context: &mut impl ParseContext,
        problem: TruthTestProblem,
    ) -> Result<(), AnalysisError> {
        self.types = InputTypeConstraint::types([]);
        context.report_analysis_problem(AnalysisProblem::truth_test(problem))
    }

    // PostgreSQL boolin accepts unique case-insensitive prefixes of these
    // words, plus 0/1, with ASCII whitespace at either end. "o" is ambiguous.
    pub(crate) fn valid_boolean_input(text: &str) -> bool {
        let text = text.trim_matches([' ', '\t', '\n', '\r', '\u{000b}', '\u{000c}']);
        matches!(
            text.to_ascii_lowercase().as_str(),
            "t" | "tr"
                | "tru"
                | "true"
                | "f"
                | "fa"
                | "fal"
                | "fals"
                | "false"
                | "y"
                | "ye"
                | "yes"
                | "n"
                | "no"
                | "on"
                | "of"
                | "off"
                | "1"
                | "0"
        )
    }

    /// Analyzed operand, including its boolean coercion when valid.
    #[must_use]
    pub fn argument(&self) -> &SemScalarExpr {
        &self.argument
    }

    /// Tested truth value; `None` denotes SQL UNKNOWN.
    #[must_use]
    pub const fn value(&self) -> Option<bool> {
        self.value
    }

    /// Whether this is an IS NOT truth predicate.
    #[must_use]
    pub const fn negated(&self) -> bool {
        self.negated
    }

    /// Boolean result domain, or an empty domain for impossible inputs.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
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

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &impl ParseContext,
    ) {
        let (node, types) = parameters.truth_test(context, &self.argument, &self.types);
        self.node = Some(node);
        self.types = types;
    }
}

#[cfg(test)]
mod test_parameter_constraints {

    use crate::sem::{CastContext, ParseContext, PgBuiltInType};

    use crate::test_helpers::query::expression;

    use crate::test_helpers::statements::query_postgres as query;

    use crate::test_helpers::truth_test::context;

    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::plain("SELECT $1 IS TRUE")]
    #[case::negated("SELECT $1 IS NOT UNKNOWN")]
    #[case::repeated("SELECT $1 IS FALSE, $1 IS NOT TRUE")]
    #[case::cast_first("SELECT $1::boolean, $1 IS TRUE")]
    #[case::cast_last("SELECT $1 IS TRUE, $1::boolean")]
    #[case::nested_test("SELECT ($1 IS TRUE) IS FALSE")]
    #[case::case_operand("SELECT (CASE WHEN true THEN $1 ELSE false END) IS TRUE")]
    #[case::coalesce_operand("SELECT COALESCE($1,false) IS TRUE")]
    #[case::case_result("SELECT CASE WHEN true THEN $1 IS TRUE ELSE false END")]
    #[case::case_condition("SELECT CASE WHEN $1 IS TRUE THEN true ELSE false END")]
    #[case::coalesce_result("SELECT COALESCE($1 IS TRUE,false)")]
    #[case::cte("WITH c AS (SELECT $1 IS TRUE AS flag) SELECT flag FROM c")]
    #[case::derived("SELECT flag FROM (SELECT $1 IS TRUE AS flag) c")]
    fn propagates_boolean_parameter_requirements(#[case] sql: &str) -> testresult::TestResult {
        let (_, context) = query(sql, context(None))?;
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        Ok(())
    }

    #[rstest::rstest]
    #[case::first("SELECT $1 IS TRUE, $1 = 1")]
    #[case::last("SELECT $1 = 1, $1 IS TRUE")]
    fn conflicting_parameter_uses_have_no_admissible_assignment(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(sql, context(None))?;
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
        );
        Ok(())
    }

    #[test]
    fn implicit_source_parameters_keep_their_source_domains() -> testresult::TestResult {
        let (query, context) = query("SELECT $1 IS TRUE", context(Some(CastContext::implicit())))?;
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        let encoded =
            ::serde_json::to_value(expression(&query).as_truth_test().unwrap().argument())?;
        assert!(
            encoded["ImplicitCast"]["value"].get("Param").is_some(),
            "{encoded}"
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::coalesce("SELECT COALESCE($1,$2) IS TRUE")]
    #[case::case("SELECT (CASE WHEN true THEN $1 ELSE $2 END) IS NOT UNKNOWN")]
    fn joint_parameters_are_constrained_through_operand(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(sql, context(None))?;
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::bool()),
            (2, PgBuiltInType::bool()),
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::bool()),
            (2, PgBuiltInType::int4()),
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::text()),
            (2, PgBuiltInType::text()),
        ])));
        assert_eq!(
            expression(&query)
                .as_truth_test()
                .unwrap()
                .argument()
                .get_type(),
            Some(PgBuiltInType::bool())
        );
        Ok(())
    }
}

impl TruthTestExpr {
    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
        }
        if self.types.single_type().is_some() {
            self.coerce();
        }
    }

    fn coerce(&mut self) {
        if self.argument.get_type() != Some(PgBuiltInType::bool()) {
            *self.argument =
                SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::bool(), &self.argument);
        }
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }
}

impl SemScalarExprNode for TruthTestExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.types.single_type().is_some() {
            Some(true)
        } else {
            None
        }
    }
}
