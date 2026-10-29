use super::{ArithExpr, BetweenExpr};
use crate::sem::{AnalysisError, OperatorName, ParseContext, ResolvedOperator, SemScalarExpr};

impl ArithExpr {
    pub(crate) fn symmetric_between(
        context: &mut impl ParseContext,
        value: &SemScalarExpr,
        start: &SemScalarExpr,
        end: &SemScalarExpr,
        negated: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        // Match PostgreSQL's transformAExprBetween. In particular, the negative
        // predicate binds </> independently of the positive predicate's >=/<=.
        let forward = Self::between_comparisons(context, value, start, end, negated)?;
        let reverse = Self::between_comparisons(context, value, end, start, negated)?;
        Ok(if negated {
            SemScalarExpr::logical_and(&[forward, reverse])
        } else {
            SemScalarExpr::logical_or(&[forward, reverse])
        })
    }

    fn between_comparisons(
        context: &mut impl ParseContext,
        value: &SemScalarExpr,
        start: &SemScalarExpr,
        end: &SemScalarExpr,
        negated: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::bound_comparisons(context, (value, start), (value, end), negated, false)
    }

    pub(crate) fn ordinary_between(
        context: &mut impl ParseContext,
        value: &SemScalarExpr,
        start: &SemScalarExpr,
        end: &SemScalarExpr,
        negated: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::bound_comparisons(context, (value, start), (value, end), negated, true)
    }

    fn bound_comparisons(
        context: &mut impl ParseContext,
        lower_inputs: (&SemScalarExpr, &SemScalarExpr),
        upper_inputs: (&SemScalarExpr, &SemScalarExpr),
        negated: bool,
        coerce_literals: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (lower, upper) = if negated { ("<", ">") } else { (">=", "<=") };
        let lower = Self::between_comparison(
            context,
            lower,
            lower_inputs.0,
            lower_inputs.1,
            coerce_literals,
        )?;
        let upper = Self::between_comparison(
            context,
            upper,
            upper_inputs.0,
            upper_inputs.1,
            coerce_literals,
        )?;
        Ok(if negated {
            SemScalarExpr::logical_or(&[lower, upper])
        } else {
            SemScalarExpr::logical_and(&[lower, upper])
        })
    }

    fn between_comparison(
        context: &mut impl ParseContext,
        name: &str,
        left: &SemScalarExpr,
        right: &SemScalarExpr,
        coerce_literals: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let name = OperatorName::from_parts(None, name);
        let binding = ResolvedOperator::resolve(context, &name, true, true);
        let (left, right) = if coerce_literals {
            let overloads = if let Some(definition) = context.get_operator_op(&name, true, true) {
                definition.binary_overloads()
            } else {
                None
            };
            SemScalarExpr::comparison_literal_operands(
                context,
                overloads,
                left.clone(),
                right.clone(),
            )
        } else {
            (left.clone(), right.clone())
        };
        let (mut expression, _) = Self::build_op(context, &name, Some(left), Some(right))?;
        expression.set_operator_binding(binding);
        Ok(expression)
    }
}

#[cfg(test)]
mod tests {
    use crate::sem::{ParseContext, PgBuiltInType, SemScalarExpr};
    use crate::test_helpers::between::{
        comparison_bindings, context, expression, mixed_bounds_context, query, schema,
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::positive("", ">=", "<=")]
    #[case::negative("NOT ", "<", ">")]
    fn retains_comparison_bindings_and_serialized_operands(
        #[case] negation: &str,
        #[values("", "SYMMETRIC ")] symmetry: &str,
        #[case] lower: &str,
        #[case] upper: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(
            &format!("SELECT 2 {negation}BETWEEN {symmetry}3 AND 1"),
            context(&[lower, upper]),
        )?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let original = expression(&query);
        let decoded: SemScalarExpr = ::serde_json::from_str(&::serde_json::to_string(original)?)?;
        assert_eq!(&decoded, original);
        assert_eq!(decoded.get_type(), Some(PgBuiltInType::bool()));
        let bindings = comparison_bindings(&decoded)?;
        let mut expected = if symmetry.is_empty() {
            vec![lower, upper]
        } else {
            vec![lower, lower, upper, upper]
        };
        expected.sort();
        assert_eq!(bindings, expected);
        Ok(())
    }

    #[rstest::rstest]
    #[case::null_value("NULL BETWEEN 1 AND 3", PgBuiltInType::int4(), PgBuiltInType::int4())]
    #[case::null_lower("2 BETWEEN NULL AND 3", PgBuiltInType::int4(), PgBuiltInType::int4())]
    #[case::null_upper(
        "2 NOT BETWEEN 1 AND NULL",
        PgBuiltInType::int4(),
        PgBuiltInType::int4()
    )]
    #[case::all_null(
        "NULL BETWEEN NULL AND NULL",
        PgBuiltInType::text(),
        PgBuiltInType::text()
    )]
    #[case::independent_targets(
        "'2' BETWEEN 1 AND '3'::text",
        PgBuiltInType::int4(),
        PgBuiltInType::text()
    )]
    fn unknown_literals_retain_each_comparisons_coerced_operands(
        #[case] predicate: &str,
        #[case] lower: crate::sem::TypeReference,
        #[case] upper: crate::sem::TypeReference,
    ) -> testresult::TestResult {
        let (query, context) = query(
            &format!("SELECT {predicate}"),
            context(&[">=", "<=", "<", ">"]),
        )?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        assert_eq!(
            crate::test_helpers::between::comparison_argument_types(expression(&query))?,
            vec![
                [Some(lower.clone()), Some(lower)],
                [Some(upper.clone()), Some(upper)],
            ],
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::first("$1 BETWEEN {symmetry}3 AND 1")]
    #[case::lower("2 BETWEEN {symmetry}$1 AND 1")]
    #[case::upper("2 NOT BETWEEN {symmetry}3 AND $1")]
    #[case::nested("COALESCE($1, 2) BETWEEN {symmetry}3 AND 1")]
    fn constrains_all_parameter_positions(
        #[case] predicate: &str,
        #[values("", "SYMMETRIC ")] symmetry: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(
            &format!("SELECT {}", predicate.replace("{symmetry}", symmetry)),
            context(&[">=", "<=", "<", ">"]),
        )?;
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        Ok(())
    }

    #[rstest::rstest]
    #[case::positive("")]
    #[case::negative("NOT ")]
    fn preserves_correlated_parameter_assignments(
        #[case] negation: &str,
        #[values("", "SYMMETRIC ")] symmetry: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(
            &format!("SELECT $1 {negation}BETWEEN {symmetry}$2 AND $3"),
            context(&[">=", "<=", "<", ">"]),
        )?;
        let constraints = context.parameter_constraints();
        for ty in [PgBuiltInType::int4(), PgBuiltInType::text()] {
            assert!(constraints.allows(&BTreeMap::from([
                (1, ty.clone()),
                (2, ty.clone()),
                (3, ty)
            ])));
        }
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::text()),
            (3, PgBuiltInType::int4()),
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::text()),
            (2, PgBuiltInType::text()),
            (3, PgBuiltInType::int4()),
        ])));
        Ok(())
    }

    #[rstest::rstest]
    #[case::first("SELECT $1 BETWEEN {symmetry}3 AND 1, $1::boolean")]
    #[case::last("SELECT $1::boolean, $1 NOT BETWEEN {symmetry}3 AND 1")]
    fn conflicting_uses_reject_assignments(
        #[case] sql: &str,
        #[values("", "SYMMETRIC ")] symmetry: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(
            &sql.replace("{symmetry}", symmetry),
            context(&[">=", "<=", "<", ">"]),
        )?;
        for ty in [
            PgBuiltInType::bool(),
            PgBuiltInType::int4(),
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

    #[rstest::rstest]
    #[case::resolved("SELECT v.value BETWEEN {symmetry}3 AND 1 FROM vals v", "TableColumn")]
    #[case::missing(
        "SELECT missing BETWEEN {symmetry}3 AND 1 FROM vals",
        "UndefinedColumn"
    )]
    #[case::ambiguous(
        "SELECT value NOT BETWEEN {symmetry}3 AND 1 FROM vals a, vals b",
        "AmbiguousColumnRef"
    )]
    fn retains_column_lookup_outcomes(
        #[case] sql: &str,
        #[case] outcome: &str,
        #[values("", "SYMMETRIC ")] symmetry: &str,
    ) -> testresult::TestResult {
        let (query, _) = query(
            &sql.replace("{symmetry}", symmetry),
            schema("CREATE TABLE vals(value int4)")?,
        )?;
        let encoded = ::serde_json::to_string(expression(&query))?;
        assert!(encoded.contains(outcome), "{encoded}");
        Ok(())
    }

    #[rstest::rstest]
    #[case::positive("")]
    #[case::negative("NOT ")]
    fn repeated_parameter_can_use_each_comparable_type(
        #[case] negation: &str,
        #[values("", "SYMMETRIC ")] symmetry: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(
            &format!("SELECT $1 {negation}BETWEEN {symmetry}$1 AND $1"),
            context(&[">=", "<=", "<", ">"]),
        )?;
        for ty in [PgBuiltInType::int4(), PgBuiltInType::text()] {
            assert!(
                context
                    .parameter_constraints()
                    .allows(&BTreeMap::from([(1, ty)]))
            );
        }
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::positive("", &[">=", "<="])]
    #[case::negative("NOT ", &["<", ">"])]
    fn bounds_need_no_comparison_with_each_other(
        #[case] negation: &str,
        #[values("", "SYMMETRIC ")] symmetry: &str,
        #[case] operators: &[&str],
    ) -> testresult::TestResult {
        let (query, context) = query(
            &format!("SELECT 2 {negation}BETWEEN {symmetry}true AND 'end'::text"),
            mixed_bounds_context(operators),
        )?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::bool()));
        Ok(())
    }
}

impl BetweenExpr {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<SemScalarExpr, AnalysisError> {
        ArithExpr::ordinary_between(context, &self.left, &self.start, &self.end, self.not)
    }

    pub(super) fn inputs_is_not_null(&self) -> Option<bool> {
        Some(self.left.is_not_null()? && self.start.is_not_null()? && self.end.is_not_null()?)
    }
}

#[cfg(test)]
mod test_public_builders {
    use crate::sem::{ParseContext, PgBuiltInType, SemScalarExpr};
    use crate::test_helpers::between::{comparison_bindings, context};
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::positive(false, ">=", "<=")]
    #[case::negative(true, "<", ">")]
    fn public_builders_complete_analysis_without_changing_serialization(
        #[case] negated: bool,
        #[case] lower: &str,
        #[case] upper: &str,
        #[values(false, true)] parameter: bool,
    ) -> testresult::TestResult {
        use crate::sem::parameter_resolution::ParameterResolution;
        use crate::sem::{ArithExpr, ScalarConstExpr};

        let integer = |value| SemScalarExpr::new_const(ScalarConstExpr::new_integer(value));
        let mut context = context(&[lower, upper]);
        let value = if parameter {
            context.parameter(1)
        } else {
            integer(2)
        };
        let bounds = (integer(1), integer(3));
        let original = if negated {
            ArithExpr::not_between(value, bounds)
        } else {
            ArithExpr::between(value, bounds)
        };
        assert_eq!(original.get_type(), Some(PgBuiltInType::bool()));
        assert_eq!(original.is_not_null(), None);
        let encoded = ::serde_json::to_value(&original)?;
        let fields = encoded["Arith"]["Between"].as_object().unwrap();
        assert_eq!(fields.len(), 4);
        for field in ["left", "start", "end", "not"] {
            assert!(fields.contains_key(field));
        }
        let mut decoded: SemScalarExpr = ::serde_json::from_value(encoded)?;
        // Inference node IDs are deliberately not serialized.
        assert_eq!(
            ::serde_json::to_value(&decoded)?,
            ::serde_json::to_value(&original)?
        );
        let constraints = ParameterResolution::infer_expression(&mut decoded, &mut context)?;
        assert_eq!(decoded.get_type(), Some(PgBuiltInType::bool()));
        let bindings = comparison_bindings(&decoded)?;
        let mut expected = vec![lower, upper];
        expected.sort();
        assert_eq!(bindings, expected);
        if parameter {
            assert!(
                constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])),
                "{constraints:?}"
            );
            assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
        } else {
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            assert_eq!(decoded.is_not_null(), Some(false));
        }
        Ok(())
    }
}
