use crate::{
    ast::{Dialect, Name, OperatorKind},
    context::ParseContext,
    names::OperatorName,
    sem::{AnalysisError, ArithExpr, SemScalarExpr},
};

impl ArithExpr {
    pub(crate) const fn ast_is_pattern(ast: &OperatorKind) -> bool {
        matches!(
            ast,
            OperatorKind::Like
                | OperatorKind::NotLike
                | OperatorKind::ILike
                | OperatorKind::Similar
        )
    }
}

impl ArithExpr {
    pub(crate) const fn ast_is_not_like(ast: &OperatorKind) -> bool {
        matches!(ast, OperatorKind::NotLike)
    }
}

impl ArithExpr {
    pub(crate) fn ast_membership_negated(ast: &OperatorKind, name: &Name) -> Option<bool> {
        match ast {
            OperatorKind::In => Some(name.0.as_slice() == ["<>"]),
            OperatorKind::NotIn => Some(true),
            _ => None,
        }
    }
}

impl ArithExpr {
    pub(crate) const fn ast_operator_distinctness(ast: &OperatorKind) -> Option<bool> {
        match ast {
            OperatorKind::Distinct => Some(false),
            OperatorKind::NotDistinct => Some(true),
            _ => None,
        }
    }
}

impl ArithExpr {
    pub(crate) const fn ast_has_named_operator(ast: &OperatorKind) -> bool {
        matches!(
            ast,
            OperatorKind::Ordinary
                | OperatorKind::Any
                | OperatorKind::All
                | OperatorKind::Like
                | OperatorKind::NotLike
                | OperatorKind::ILike
                | OperatorKind::Similar
        )
    }
}

impl ArithExpr {
    pub(crate) const fn ast_without_catalog_operator(ast: &OperatorKind) -> bool {
        matches!(ast, OperatorKind::Between | OperatorKind::NotBetween)
    }
}

impl ArithExpr {
    pub(crate) fn from_ast_operator_kind(
        ast: &OperatorKind,
        context: &mut impl ParseContext,
        name: &OperatorName,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match ast {
            OperatorKind::Ordinary => Ok(Self::build_op(context, name, left, right)?.0),
            OperatorKind::Like
            | OperatorKind::NotLike
            | OperatorKind::ILike
            | OperatorKind::Similar => Self::pattern(context, name, left, right),
            OperatorKind::Any => Ok(Self::build_op_any_resolved(context, name, left, right)?.0),
            OperatorKind::All => Ok(Self::build_op_all_resolved(context, name, left, right)?.0),
            OperatorKind::NullIf => Ok(Self::build_null_if(context, left, right)?.0),
            OperatorKind::Between | OperatorKind::NotBetween => {
                Self::from_ast_between(ast, context, left, right)
            }
            OperatorKind::BetweenSymmetric | OperatorKind::NotBetweenSymmetric => {
                Self::from_ast_symmetric_between(ast, context, left, right)
            }
            _ => todo!("operator semantic analysis"),
        }
    }
}

#[cfg(test)]
mod test_pattern_resolution {

    use crate::{
        context::ParseContext,
        sem::{PgBuiltInType, SemScalarExpr, TypeReference as SemTypeReference},
        test_helpers::{
            membership::context as membership_context,
            pattern::{context, expression, query},
        },
    };

    use std::collections::BTreeMap;

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

impl ArithExpr {
    pub(crate) fn from_ast_between(
        ast: &OperatorKind,
        context: &mut impl ParseContext,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = left.expect("BETWEEN argument");
        let values = right.expect("BETWEEN bounds").into_list_values()?;
        let Ok([start, end]): Result<[SemScalarExpr; 2], _> = values.try_into() else {
            return AnalysisError::raise_unexpected_input("BETWEEN bounds");
        };
        if context.dialect() == Dialect::postgres() {
            return Self::ordinary_between(
                context,
                &left,
                &start,
                &end,
                matches!(ast, OperatorKind::NotBetween),
            );
        }
        Ok(if matches!(ast, OperatorKind::NotBetween) {
            Self::not_between(left, (start, end))
        } else {
            Self::between(left, (start, end))
        })
    }
}

impl ArithExpr {
    pub(crate) fn from_ast_symmetric_between(
        ast: &OperatorKind,
        context: &mut impl ParseContext,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = left.expect("BETWEEN SYMMETRIC argument");
        let values = right
            .expect("BETWEEN SYMMETRIC bounds")
            .into_list_values()?;
        let Ok([start, end]): Result<[SemScalarExpr; 2], _> = values.try_into() else {
            return AnalysisError::raise_unexpected_input("BETWEEN SYMMETRIC bounds");
        };
        Self::symmetric_between(
            context,
            &left,
            &start,
            &end,
            matches!(ast, OperatorKind::NotBetweenSymmetric),
        )
    }
}

impl ArithExpr {
    pub(crate) const fn ast_is_null_if(ast: &OperatorKind) -> bool {
        matches!(ast, OperatorKind::NullIf)
    }
}
