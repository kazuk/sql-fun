use crate::sem::CommonType;
use crate::{
    names::OperatorName,
    sem::{
        AnalysisError, InputTypeConstraint, ParseContext, ResolvedOperator, SemScalarExpr,
        parameter_resolution::ParameterResolution,
        scalar_expr::arith_expr::ArithExpr,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A parameter-dependent pattern call whose unknown literals still need SQL context.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PatternExpr {
    operands: [SemScalarExpr; 2],
    binding: ResolvedOperator,
}

impl PatternExpr {
    pub(super) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for operand in &mut self.operands {
            operand.walk(visitor)?;
        }
        Ok(())
    }
    pub(super) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for operand in &self.operands {
            operand.visit(visitor)?;
        }
        Ok(())
    }
    pub(super) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let mut operands = self.operands.clone();
        for operand in &mut operands {
            if SemScalarExpr::common_type_input(operand) == Some(CommonType::unknown()) {
                *operand = parameters.operation(
                    "unknown pattern literal",
                    std::slice::from_ref(operand),
                    &InputTypeConstraint::any(),
                    operand.is_not_null(),
                );
            }
        }
        let mut result =
            self.binding
                .infer(parameters, context, Some(&operands[0]), Some(&operands[1]))?;
        if let Some(value) = &mut result {
            value.retain_operator_binding(&self.binding);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod test_postgres_unknown_pattern_literals_follow_parameter_context {
    use crate::{
        context::ParseContext,
        sem::{PgBuiltInType, TypeReference as SemTypeReference},
        test_helpers::pattern::{context, expression, query},
    };

    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::pattern_literal("SELECT $1 LIKE 'a%'")]
    #[case::value_literal("SELECT 'a' NOT LIKE $1")]
    #[case::null_literal("SELECT NULL LIKE $1")]
    fn postgres_unknown_pattern_literals_follow_parameter_context(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(sql, context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        for ty in [
            PgBuiltInType::text(),
            SemTypeReference::from_master_type_name("bytea"),
        ] {
            assert!(
                context
                    .parameter_constraints()
                    .allows(&BTreeMap::from([(1, ty)]))
            );
        }
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::int4())]))
        );
        assert!(
            expression(&query)
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::like("LIKE")]
    #[case::not_like("NOT LIKE")]
    fn postgres_escape_constraints_preserve_correlated_overloads(
        #[case] operator: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(&format!("SELECT $1 {operator} $2 ESCAPE $3"), context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        for ty in [
            PgBuiltInType::text(),
            SemTypeReference::from_master_type_name("bytea"),
        ] {
            assert!(context.parameter_constraints().allows(&BTreeMap::from([
                (1, ty.clone()),
                (2, ty.clone()),
                (3, ty),
            ])));
        }
        assert!(!context.parameter_constraints().allows(&BTreeMap::from([
            (1, PgBuiltInType::text()),
            (2, crate::sem::TypeReference::from_master_type_name("bytea")),
            (3, PgBuiltInType::text()),
        ])));
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(3, PgBuiltInType::int4())]))
        );
        assert!(
            expression(&query)
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::ilike("SELECT $1 ILIKE $2 ESCAPE $3")]
    #[case::similar("SELECT $1 SIMILAR TO $2 ESCAPE $3")]
    #[case::repeat("SELECT $1 LIKE $1 ESCAPE $1")]
    #[case::cast("SELECT $1 LIKE $2 ESCAPE $3::text")]
    fn postgres_pattern_constraints_visit_every_operand(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(sql, context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        Ok(())
    }

    #[test]
    fn postgres_pattern_conflicting_use_is_not_accepted() -> testresult::TestResult {
        let (_, context) = query("SELECT $1 LIKE 'a%', NOT $1", context())?;
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| problem.code() == "parameter_constraint")
        );
        assert!(
            !context
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
}

impl PatternExpr {
    pub(super) const fn binding(&self) -> &ResolvedOperator {
        &self.binding
    }
    pub(super) fn set_binding(&mut self, binding: ResolvedOperator) {
        self.binding = binding;
    }
}

impl ArithExpr {
    pub(crate) fn pattern(
        context: &mut impl ParseContext,
        name: &OperatorName,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (Some(left), Some(right)) = (left, right) else {
            return AnalysisError::raise_unexpected_input("pattern operands");
        };
        let unknown = [&left, &right]
            .into_iter()
            .any(|value| SemScalarExpr::common_type_input(value) == Some(CommonType::unknown()));
        if unknown && (left.requires_argument_type() || right.requires_argument_type()) {
            let binding = ResolvedOperator::resolve(context, name, true, true);
            return Ok(SemScalarExpr::arithmetic(Self::Pattern(Box::new(
                PatternExpr {
                    operands: [left, right],
                    binding,
                },
            ))));
        }
        let overloads = if let Some(definition) = context.get_operator(name, true, true) {
            definition.binary_overloads()
        } else {
            None
        };
        let (left, right) =
            SemScalarExpr::comparison_literal_operands(context, overloads, left, right);
        Ok(Self::build_op(context, name, Some(left), Some(right))?.0)
    }
}

#[cfg(test)]
mod test_postgres_patterns_retain_bound_operators {
    use crate::{
        sem::{PgBuiltInType, SemScalarExpr},
        test_helpers::{
            membership::context as membership_context,
            pattern::{context, expression, query},
        },
    };

    #[rstest::rstest]
    #[case::like("LIKE", "~~")]
    #[case::not_like("NOT LIKE", "!~~")]
    #[case::ilike("ILIKE", "~~*")]
    #[case::not_ilike("NOT ILIKE", "!~~*")]
    #[case::similar("SIMILAR TO", "~")]
    #[case::not_similar("NOT SIMILAR TO", "!~")]
    fn postgres_patterns_retain_bound_operators(
        #[case] operator: &str,
        #[case] name: &str,
        #[values("", " ESCAPE '!'")] escape: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(&format!("SELECT 'a' {operator} 'a'{escape}"), context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let value = expression(&query);
        assert!(value.operator_binding().unwrap().resolution().is_resolved());
        assert_eq!(
            crate::test_helpers::between::comparison_bindings(value)?,
            vec![name]
        );
        let encoded = ::serde_json::to_value(value)?;
        let decoded: SemScalarExpr = ::serde_json::from_value(encoded.clone())?;
        assert_eq!(::serde_json::to_value(&decoded)?, encoded);
        assert_eq!(decoded.get_type(), Some(PgBuiltInType::bool()));
        if !escape.is_empty() || operator.contains("SIMILAR") {
            let helper = value
                .binary_argument(1)
                .unwrap()
                .as_function_call()
                .unwrap();
            assert!(helper.name_resolution().is_resolved());
            let name = if operator.contains("SIMILAR") {
                "similar_to_escape"
            } else {
                "like_escape"
            };
            assert_eq!(
                helper.name(),
                &crate::names::FullName::with_schema("pg_catalog", name)
            );
        }
        Ok(())
    }

    #[test]
    fn postgres_missing_pattern_operator_retains_failed_binding() -> testresult::TestResult {
        let (query, context) = query("SELECT 'a' LIKE 'a'", membership_context())?;
        assert!(
            !expression(&query)
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        assert_eq!(expression(&query).get_type(), None);
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| problem.code() == "undefined_operator")
        );
        Ok(())
    }
}
