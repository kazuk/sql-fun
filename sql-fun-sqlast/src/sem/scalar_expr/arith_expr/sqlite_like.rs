use crate::{
    context::ParseContext as ContextParseContext,
    sem::{
        AnalysisError, PgBuiltInType, SemScalarExpr, TypeReference,
        scalar_expr::{SemScalarExprNode, arith_expr::ArithExpr},
        traversal::{SemanticVisitor, SemanticVisitorMut, optional, visit_optional},
    },
};

/// SQLite's builtin LIKE predicate and its analyzed operands.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SqliteLikeExpr {
    value: SemScalarExpr,
    pattern: SemScalarExpr,
    escape: Option<SemScalarExpr>,
    negated: bool,
}

impl SqliteLikeExpr {
    /// Analyzed value being matched.
    pub const fn value(&self) -> &SemScalarExpr {
        &self.value
    }
    /// Analyzed pattern.
    pub const fn pattern(&self) -> &SemScalarExpr {
        &self.pattern
    }
    /// Explicit escape operand; absent when ESCAPE was omitted.
    pub const fn escape(&self) -> Option<&SemScalarExpr> {
        self.escape.as_ref()
    }
    /// Whether the predicate is NOT LIKE.
    pub const fn negated(&self) -> bool {
        self.negated
    }

    pub(super) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.value.walk(visitor)?;
        self.pattern.walk(visitor)?;
        optional(&mut self.escape, visitor)
    }
    pub(super) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.value.visit(visitor)?;
        self.pattern.visit(visitor)?;
        visit_optional(&self.escape, visitor)
    }
}

impl SemScalarExprNode for SqliteLikeExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(PgBuiltInType::int4())
    }
    fn is_not_null(&self) -> Option<bool> {
        // TODO: Resolve user-defined overrides of both like(pattern, value) and
        // like(pattern, value, escape), including their return/nullability contracts.
        // sqlite3_create_function() can replace either arity independently; input
        // nullability cannot establish the result's nullability without that binding.
        None
    }
}

impl ArithExpr {
    pub(crate) fn sqlite_like(
        context: &mut impl ContextParseContext,
        value: &SemScalarExpr,
        pattern: &SemScalarExpr,
        escape: Option<&SemScalarExpr>,
        negated: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        value.validate_scalar_value(context)?;
        pattern.validate_scalar_value(context)?;
        if let Some(escape) = escape {
            escape.validate_scalar_value(context)?;
        }
        Ok(SemScalarExpr::arithmetic(Self::SqliteLike(Box::new(
            SqliteLikeExpr {
                value: value.clone(),
                pattern: pattern.clone(),
                escape: escape.cloned(),
                negated,
            },
        ))))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use crate::ast::Dialect;
    use crate::sem::ParseContext;
    use crate::sem::PgBuiltInType;
    use crate::sem::SemScalarExpr;
    use crate::test_helpers::membership::context_for;
    use crate::test_helpers::membership::expression;
    use crate::test_helpers::statements::query_in_context as query;
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::plain("SELECT 'abc' LIKE 'a%'", Some(1), false, false)]
    #[case::negative("SELECT 'abc' NOT LIKE 'a%'", Some(0), true, false)]
    #[case::escape("SELECT 'a_' LIKE 'a!_' ESCAPE '!'", Some(1), false, true)]
    #[case::numeric("SELECT 123 LIKE '12%'", Some(1), false, false)]
    #[case::null_value("SELECT NULL LIKE 'a%'", None, false, false)]
    #[case::null_pattern("SELECT 'a' NOT LIKE NULL", None, true, false)]
    #[case::null_escape("SELECT 'a' LIKE 'a' ESCAPE NULL", None, false, true)]
    fn sqlite_like_retains_semantics(
        #[case] sql: &str,
        #[case] expected: Option<i64>,
        #[case] negated: bool,
        #[case] escape: bool,
    ) -> testresult::TestResult {
        let db = ::rusqlite::Connection::open_in_memory()?;
        assert_eq!(
            db.query_row(sql, [], |row| row.get::<_, Option<i64>>(0))?,
            expected
        );
        let (query, context) = query(sql, context_for(Dialect::sqlite()))?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let expression = expression(&query);
        assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
        // Even non-NULL constants cannot prove a replacement like() is non-NULL.
        assert_eq!(expression.is_not_null(), None);
        assert!(expression.operator_binding().is_none());
        let encoded = ::serde_json::to_value(expression)?;
        let decoded: SemScalarExpr = ::serde_json::from_value(encoded.clone())?;
        assert_eq!(::serde_json::to_value(&decoded)?, encoded);
        let predicate = decoded.as_sqlite_like().unwrap();
        assert_eq!(predicate.negated(), negated);
        assert_eq!(predicate.escape().is_some(), escape);
        assert!(predicate.value().as_const().is_some());
        assert!(predicate.pattern().as_const().is_some());
        Ok(())
    }

    #[test]
    fn sqlite_like_visits_all_parameters_without_imposing_affinity() -> testresult::TestResult {
        let (query, context) = query(
            "SELECT $1 NOT LIKE $2 ESCAPE $3",
            context_for(Dialect::sqlite()),
        )?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        assert!(context.parameter_constraints().allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::text()),
            (3, PgBuiltInType::bool()),
        ])));
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(4, PgBuiltInType::text())]))
        );
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(expression(&query).is_not_null(), None);
        assert!(
            expression(&query)
                .as_sqlite_like()
                .unwrap()
                .escape()
                .is_some()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::value("SELECT (1, 2) LIKE 'a%'")]
    #[case::pattern("SELECT 'a' LIKE (1, 2)")]
    #[case::escape("SELECT 'a' LIKE 'a' ESCAPE (1, 2)")]
    fn sqlite_like_requires_scalar_operands(#[case] sql: &str) -> testresult::TestResult {
        let db = ::rusqlite::Connection::open_in_memory()?;
        assert!(db.prepare(sql).is_err());
        let (query, context) = query(sql, context_for(Dialect::sqlite()))?;
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| problem.code() == "query_scalar_value_required"),
            "{:?}",
            context.problems()
        );
        assert!(expression(&query).as_sqlite_like().is_some());
        Ok(())
    }

    #[test]
    fn sqlite_like_binds_escape_column() -> testresult::TestResult {
        let (query, context) = query(
            "SELECT v LIKE p ESCAPE e FROM (SELECT 'a' AS v, 'a' AS p, '!' AS e)",
            context_for(Dialect::sqlite()),
        )?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let value = expression(&query).as_sqlite_like().unwrap();
        assert!(value.value().get_column_def().is_some());
        assert!(value.pattern().get_column_def().is_some());
        assert!(value.escape().unwrap().get_column_def().is_some());
        Ok(())
    }
}

impl ArithExpr {
    /// SQLite builtin LIKE semantics, with no fabricated catalog operator binding.
    pub(crate) const fn as_sqlite_like(&self) -> Option<&SqliteLikeExpr> {
        match self {
            Self::SqliteLike(value) => Some(value),
            _ => None,
        }
    }
}
