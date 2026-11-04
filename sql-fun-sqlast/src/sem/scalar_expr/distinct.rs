use crate::{
    ast::Dialect,
    names::OperatorName,
    sem::{
        AnalysisError, AnalysisProblem, ArithExpr, InputTypeConstraint, MinMaxExpr,
        ParameterConstraints, ParseContext, PgBuiltInType, ResolvedOperator, SemScalarExpr,
        TypeReference,
        parameter_resolution::ParameterResolution,
        problem::DistinctProblem,
        scalar_expr::{ScalarConstExpr, SemScalarExprNode},
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A null-safe comparison, distinct from the equality operation it uses.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DistinctExpr {
    comparison: DistinctComparison,
    negated: bool,
    types: InputTypeConstraint,
    not_null: Option<bool>,
    #[serde(skip)]
    node: Option<usize>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum DistinctComparison {
    Equality(Box<SemScalarExpr>),
    Deferred(Box<DeferredEquality>),
    // Bare NULL and SQLite comparisons do not require catalog equality.
    Operands(Vec<SemScalarExpr>),
    Rows(Vec<SemScalarExpr>),
    Invalid(Vec<SemScalarExpr>),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct DeferredEquality {
    operands: [SemScalarExpr; 2],
    binding: ResolvedOperator,
}

impl DeferredEquality {
    fn infer(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut operands = self.operands.clone();
        for operand in &mut operands {
            if MinMaxExpr::input_type(operand) == Some(MinMaxExpr::unknown()) {
                *operand = parameters.operation(
                    "unknown comparison literal",
                    std::slice::from_ref(operand),
                    &InputTypeConstraint::any(),
                    operand.is_not_null(),
                );
            }
        }
        let mut equality = self
            .binding
            .infer(parameters, context, Some(&operands[0]), Some(&operands[1]))?
            .expect("deferred comparison has constraint inputs");
        equality.retain_operator_binding(&self.binding);
        Ok(equality)
    }
}

impl DistinctComparison {
    fn equality(value: SemScalarExpr) -> Self {
        Self::Equality(Box::new(value))
    }
    fn operands(left: SemScalarExpr, right: SemScalarExpr) -> Self {
        Self::Operands(vec![left, right])
    }
    fn invalid(left: SemScalarExpr, right: SemScalarExpr) -> Self {
        Self::Invalid(vec![left, right])
    }
    const fn rows(values: Vec<SemScalarExpr>) -> Self {
        Self::Rows(values)
    }
    fn deferred(left: SemScalarExpr, right: SemScalarExpr, binding: ResolvedOperator) -> Self {
        Self::Deferred(Box::new(DeferredEquality {
            operands: [left, right],
            binding,
        }))
    }
    fn operand(&self, index: usize) -> Option<&SemScalarExpr> {
        match self {
            Self::Equality(value) => value.binary_argument(index),
            Self::Deferred(value) => value.operands.get(index),
            Self::Operands(values) | Self::Invalid(values) => values.get(index),
            Self::Rows(_) => None,
        }
    }
    fn as_equality(&self) -> Option<&SemScalarExpr> {
        if let Self::Equality(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn infer(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Self::Deferred(value) = self {
            *self = Self::equality(value.infer(parameters, context)?);
        }
        Ok(())
    }
    fn row_nullability(&self) -> Option<Option<bool>> {
        if let Self::Rows(values) = self {
            Some(
                if values.iter().all(|value| value.is_not_null() == Some(true)) {
                    Some(true)
                } else {
                    None
                },
            )
        } else {
            None
        }
    }
    fn has_impossible_field(&self) -> bool {
        if let Self::Rows(values) = self {
            values.iter().any(|value| {
                value
                    .as_distinct()
                    .is_some_and(|value| value.types.admissible_types() == Some(&[]))
            })
        } else {
            false
        }
    }

    fn expressions(&self) -> &[SemScalarExpr] {
        match self {
            Self::Equality(value) => std::slice::from_ref(value),
            Self::Deferred(value) => &value.operands,
            Self::Operands(values) | Self::Rows(values) | Self::Invalid(values) => values,
        }
    }

    fn walk<V: SemanticVisitorMut>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Equality(value) => value.walk(visitor),
            Self::Deferred(value) => {
                for operand in &mut value.operands {
                    operand.walk(visitor)?;
                }
                Ok(())
            }
            Self::Operands(values) | Self::Rows(values) | Self::Invalid(values) => {
                for value in values {
                    value.walk(visitor)?;
                }
                Ok(())
            }
        }
    }
    fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Equality(value) => value.visit(visitor),
            Self::Deferred(value) => {
                for operand in &value.operands {
                    operand.visit(visitor)?;
                }
                Ok(())
            }
            Self::Operands(values) | Self::Rows(values) | Self::Invalid(values) => {
                for value in values {
                    value.visit(visitor)?;
                }
                Ok(())
            }
        }
    }

    const fn requires_boolean(&self) -> bool {
        matches!(self, Self::Equality(_) | Self::Deferred(_) | Self::Rows(_))
    }
}

impl DistinctExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        left: SemScalarExpr,
        right: SemScalarExpr,
        negated: bool,
    ) -> Result<Self, AnalysisError> {
        if context.dialect() == Dialect::sqlite()
            && left.is_membership_row() != right.is_membership_row()
        {
            left.validate_scalar_value(context)?;
            right.validate_scalar_value(context)?;
        }
        let mut result = Self {
            comparison: DistinctComparison::rows(Vec::new()),
            negated,
            types: InputTypeConstraint::exact(&PgBuiltInType::bool()),
            node: None,
            not_null: Some(true),
        };
        result.comparison = if context.dialect() == Dialect::sqlite()
            || left.as_const().is_some_and(ScalarConstExpr::is_null)
            || right.as_const().is_some_and(ScalarConstExpr::is_null)
        {
            DistinctComparison::operands(left, right)
        } else if left.is_row_constructor() && right.is_row_constructor() {
            if let Some(comparisons) = left.distinct_row_comparisons(context, &right)? {
                DistinctComparison::rows(comparisons)
            } else {
                result.types = InputTypeConstraint::types([]);
                DistinctComparison::invalid(left, right)
            }
        } else {
            result.resolve_equality(context, left, right)?
        };
        if let Some(not_null) = result.comparison.row_nullability() {
            result.not_null = not_null;
        }
        if result.comparison.has_impossible_field() {
            result.types = InputTypeConstraint::types([]);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod test_resolve {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::distinct as test_helpers;

    #[cfg(feature = "sqlite")]
    use crate::ast::Dialect;

    use crate::{
        sem::{CastContext, CastDefinition, ScalarConstExpr, scalar_expr::distinct::DistinctExpr},
        test_helpers::TestParseContext,
    };

    use crate::sem::{ParseContext, PgBuiltInType, SemScalarExpr};

    use crate::test_helpers::distinct::context;

    use crate::test_helpers::distinct::schema;

    use crate::test_helpers::query::expression;

    use crate::test_helpers::statements::query_in_context as query;

    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::distinct("DISTINCT", false)]
    #[case::not_distinct("NOT DISTINCT", true)]
    fn retains_equality_binding_and_polarity(
        #[case] kind: &str,
        #[case] negated: bool,
        #[values("1", "$1")] operand: &str,
    ) -> testresult::TestResult {
        let (query, _) = query(&format!("SELECT {operand} IS {kind} FROM 1"), context())?;
        let expr = expression(&query);
        let distinct = expr.as_distinct().unwrap();
        assert_eq!(distinct.negated(), negated);
        assert!(
            distinct
                .equality()
                .unwrap()
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        let encoded = ::serde_json::to_string(expr)?;
        let decoded: SemScalarExpr = ::serde_json::from_str(&encoded)?;
        assert_eq!(decoded.as_distinct().unwrap().negated(), negated);
        assert!(
            decoded
                .as_distinct()
                .unwrap()
                .equality()
                .unwrap()
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::left("SELECT NULL IS DISTINCT FROM $1")]
    #[case::right("SELECT $1 IS NOT DISTINCT FROM NULL")]
    fn bare_null_does_not_impose_equality_or_type(#[case] sql: &str) -> testresult::TestResult {
        let (query, context) = query(sql, context())?;
        let test = expression(&query).as_distinct().unwrap();
        assert!(test.equality().is_none());
        assert_eq!(test.comparisons().len(), 2);
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
        Ok(())
    }

    #[rstest::rstest]
    #[case::arity("SELECT ROW(1,2) IS DISTINCT FROM ROW(1)", "distinct_row_arity")]
    #[case::incompatible(
        "SELECT 1 IS DISTINCT FROM true",
        "binary_operator_overload_resolution_failed"
    )]
    fn retains_invalid_comparisons(
        #[case] sql: &str,
        #[case] code: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(sql, context())?;
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| problem.code() == code)
        );
        assert!(expression(&query).as_distinct().is_some());
        Ok(())
    }

    #[test]
    fn binds_columns_and_retains_null_literal_row_distinction() -> testresult::TestResult {
        let context = schema("CREATE TABLE flags(nullable boolean, required boolean NOT NULL)")?;
        let (query, _) = query(
            "SELECT nullable IS DISTINCT FROM required, ROW(NULL,NULL) IS DISTINCT FROM NULL FROM flags",
            context,
        )?;
        let test = expression(&query).as_distinct().unwrap();
        assert!(
            test.equality()
                .unwrap()
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        let test = query
            .result_columns()
            .get_column_at(1)
            .unwrap()
            .expression()
            .unwrap()
            .as_distinct()
            .unwrap();
        assert!(test.comparisons()[0].is_row_constructor());
        assert!(test.equality().is_none());
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::distinct("1 IS DISTINCT FROM NULL", false, 1, false)]
    #[case::not_distinct("NULL IS NOT DISTINCT FROM NULL", true, 1, false)]
    #[case::is("1 IS 1", true, 1, false)]
    #[case::is_not("1 IS NOT 1", false, 0, false)]
    #[case::mixed("'1' IS DISTINCT FROM 1", false, 1, false)]
    #[case::truth("2 IS TRUE", true, 1, false)]
    #[case::truth_distinct("2 IS NOT DISTINCT FROM TRUE", true, 1, false)]
    #[case::collation("'A' COLLATE NOCASE IS NOT DISTINCT FROM 'a'", true, 1, true)]
    fn sqlite_predicates_preserve_operands_without_catalog_equality(
        #[case] sql: &str,
        #[case] negated: bool,
        #[case] oracle: i32,
        #[case] missing_collation: bool,
    ) -> testresult::TestResult {
        let sql = format!("SELECT {sql}");
        let connection = ::rusqlite::Connection::open_in_memory()?;
        assert_eq!(
            connection.query_row(&sql, [], |row| row.get::<_, i32>(0))?,
            oracle
        );
        let context = test_helpers::context_for(Dialect::sqlite());
        let (query, context) = query(&sql, context)?;
        let test = expression(&query).as_distinct().unwrap();
        assert_eq!(test.negated(), negated);
        assert!(test.equality().is_none());
        assert_eq!(test.comparisons().len(), 2);
        if missing_collation {
            assert_eq!(context.problems().len(), 1);
            assert_eq!(context.problems()[0].code(), "collation_not_found");
            assert!(
                test.comparisons()[0]
                    .as_collate()
                    .unwrap()
                    .collation()
                    .is_missing()
            );
        } else {
            assert!(context.problems().is_empty(), "{:?}", context.problems());
        }
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_comparisons_do_not_constrain_input_affinity() -> testresult::TestResult {
        let context = test_helpers::context_for(Dialect::sqlite());
        let (_, context) = query("SELECT ?1 IS DISTINCT FROM 1, ?1 IS NOT NULL", context)?;
        for ty in [
            PgBuiltInType::int4(),
            PgBuiltInType::text(),
            PgBuiltInType::bool(),
        ] {
            assert!(
                context
                    .parameter_constraints()
                    .allows(&BTreeMap::from([(1, ty)]))
            );
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::non_strict(false)]
    #[case::strict(true)]
    fn custom_equality_strictness_does_not_guarantee_non_null(
        #[case] strict: bool,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        context.setup_binary("=", "int4", "int4", "bool", strict);
        let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let predicate = SemScalarExpr::distinct(DistinctExpr::resolve(
            &mut context,
            value.clone(),
            value,
            false,
        )?);
        assert_eq!(predicate.is_not_null(), None);
        assert_eq!(predicate.get_type(), Some(PgBuiltInType::bool()));
        Ok(())
    }

    #[test]
    fn non_boolean_equality_is_rejected_and_retained() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        context.setup_binary("=", "int4", "int4", "int4", true);
        let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let predicate = DistinctExpr::resolve(&mut context, value.clone(), value, false)?;
        assert!(context.has_reported_problem("distinct_equality_result"));
        assert_eq!(predicate.types().admissible_types(), Some([].as_slice()));
        assert_eq!(
            predicate.equality().unwrap().get_type(),
            Some(PgBuiltInType::int4())
        );
        Ok(())
    }

    #[test]
    fn retains_implicit_equality_coercions() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::int8());
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &PgBuiltInType::int8(),
            Some(CastDefinition::new(CastContext::implicit())),
        );
        context.setup_binary("=", "int8", "int8", "bool", true);
        let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let predicate = DistinctExpr::resolve(&mut context, value.clone(), value, false)?;
        assert_eq!(
            predicate.left().unwrap().get_type(),
            Some(PgBuiltInType::int8())
        );
        assert_eq!(
            predicate.right().unwrap().get_type(),
            Some(PgBuiltInType::int8())
        );
        Ok(())
    }

    #[test]
    fn missing_equality_retains_failed_binding() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let predicate = DistinctExpr::resolve(&mut context, value.clone(), value, false)?;
        assert!(context.has_reported_problem("undefined_operator"));
        assert!(
            !predicate
                .equality()
                .unwrap()
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        assert_eq!(predicate.types().admissible_types(), Some([].as_slice()));
        assert!(predicate.left().is_some());
        Ok(())
    }
}

impl DistinctExpr {
    fn resolve_equality(
        &mut self,
        context: &mut impl ParseContext,
        left: SemScalarExpr,
        right: SemScalarExpr,
    ) -> Result<DistinctComparison, AnalysisError> {
        let name = OperatorName::from_parts(None, "=");
        self.not_null = context.resolved_operator_result_is_not_null(&name);
        let binding = ResolvedOperator::resolve_comparison(context, &name);
        if !binding.resolution().is_resolved() {
            self.types = InputTypeConstraint::types([]);
        }
        let has_unknown = [&left, &right]
            .into_iter()
            .any(|value| MinMaxExpr::input_type(value) == Some(MinMaxExpr::unknown()));
        let deferred = left.requires_argument_type() || right.requires_argument_type();
        if has_unknown && deferred {
            return Ok(DistinctComparison::deferred(left, right, binding));
        }

        let overloads = if let Some(definition) = context.get_operator(&name, true, true) {
            definition.binary_overloads()
        } else {
            None
        };
        let (left, right) =
            SemScalarExpr::comparison_literal_operands(context, overloads, left, right);
        let mut equality = ArithExpr::build_op(context, &name, Some(left), Some(right))?.0;
        equality.set_operator_binding(binding);
        if let Some(ty) = equality.get_type() {
            if ty != PgBuiltInType::bool() {
                context.report_analysis_problem(AnalysisProblem::distinct(
                    DistinctProblem::equality_result(&ty),
                ))?;
                self.types = InputTypeConstraint::types([]);
            }
        } else if !deferred {
            self.types = InputTypeConstraint::types([]);
        }
        Ok(DistinctComparison::equality(equality))
    }

    /// Scalar left operand, including equality coercions after analysis.
    /// Explicit row pairs expose ordered field predicates through `comparisons`.
    #[must_use]
    pub fn left(&self) -> Option<&SemScalarExpr> {
        self.comparison.operand(0)
    }
    /// Scalar right operand, including equality coercions after analysis.
    /// Explicit row pairs expose ordered field predicates through `comparisons`.
    #[must_use]
    pub fn right(&self) -> Option<&SemScalarExpr> {
        self.comparison.operand(1)
    }

    /// Underlying PostgreSQL equality, including its binding and coercions.
    /// Bare NULL, explicit row pairs, and SQLite have no scalar equality call.
    #[must_use]
    pub fn equality(&self) -> Option<&SemScalarExpr> {
        self.comparison.as_equality()
    }

    /// Retained comparison expressions: one equality, two direct operands,
    /// or ordered field predicates for a pair of explicit rows.
    #[must_use]
    pub fn comparisons(&self) -> &[SemScalarExpr] {
        self.comparison.expressions()
    }

    /// Whether this is IS NOT DISTINCT FROM (SQLite IS).
    #[must_use]
    pub const fn negated(&self) -> bool {
        self.negated
    }

    /// Admissible boolean result domain; empty when inputs are impossible.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }

    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.comparison.walk(visitor)
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.comparison.visit(visitor)
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.comparison.infer(parameters, context)?;
        self.node = Some(parameters.distinct_result(
            self.comparison.expressions(),
            self.comparison.requires_boolean(),
            &self.types,
        ));
        Ok(())
    }
}

#[cfg(test)]
mod test_parameter_constraints {

    use crate::sem::{ParseContext, PgBuiltInType};

    use crate::test_helpers::distinct::context;

    use crate::test_helpers::statements::query_in_context as query;

    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::simple("SELECT $1 IS DISTINCT FROM 1")]
    #[case::repeated("SELECT $1 IS NOT DISTINCT FROM 1, $1 IS DISTINCT FROM 2")]
    #[case::nested("SELECT ($1 IS DISTINCT FROM 1) IS NOT DISTINCT FROM false")]
    #[case::row("SELECT ROW($1, NULL) IS DISTINCT FROM ROW(1, NULL)")]
    #[case::case_result("SELECT CASE WHEN true THEN $1 IS DISTINCT FROM 1 ELSE false END")]
    #[case::derived("SELECT flag FROM (SELECT $1 IS DISTINCT FROM 1 AS flag) c")]
    fn constrains_comparison_inputs_without_replacing_predicate(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(sql, context())?;
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
        assert_eq!(
            constraints.result_column(0).unwrap().single_type(),
            Some(PgBuiltInType::bool())
        );
        Ok(())
    }

    #[test]
    fn parameter_assignments_are_correlated() -> testresult::TestResult {
        let (_, context) = query("SELECT $1 IS DISTINCT FROM $2", context())?;
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::int4())
        ])));
        assert!(constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::bool()),
            (2, PgBuiltInType::bool())
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::bool())
        ])));
        Ok(())
    }

    #[test]
    fn conflicting_comparisons_have_no_admissible_assignment() -> testresult::TestResult {
        let (_, context) = query(
            "SELECT $1 IS DISTINCT FROM 1, $1 IS NOT DISTINCT FROM false",
            context(),
        )?;
        for ty in [
            PgBuiltInType::int4(),
            PgBuiltInType::bool(),
            PgBuiltInType::text(),
        ] {
            assert!(
                !context
                    .parameter_constraints()
                    .allows(&BTreeMap::from([(1, ty)]))
            );
        }
        Ok(())
    }
}

impl DistinctExpr {
    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
        }
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }
}

impl SemScalarExprNode for DistinctExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        if self.types.single_type().is_some() {
            self.not_null
        } else {
            None
        }
    }
}

impl super::RowExpr {
    pub(super) fn distinct_comparisons(
        &self,
        context: &mut impl ParseContext,
        other: &Self,
    ) -> Result<Option<Vec<SemScalarExpr>>, AnalysisError> {
        if self.0.len() != other.0.len() {
            context.report_analysis_problem(AnalysisProblem::distinct(
                DistinctProblem::row_arity(self.0.len(), other.0.len()),
            ))?;
            return Ok(None);
        }
        let mut comparisons = Vec::new();
        for (left, right) in self.0.iter().zip(&other.0) {
            comparisons.push(SemScalarExpr::distinct(DistinctExpr::resolve(
                context,
                left.clone(),
                right.clone(),
                false,
            )?));
        }
        Ok(Some(comparisons))
    }
}
