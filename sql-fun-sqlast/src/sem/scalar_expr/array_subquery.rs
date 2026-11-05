use crate::{
    ast::Dialect,
    sem::{
        AnalysisError, AnalysisProblem, InputTypeConstraint, ParameterConstraints, ParseContext,
        PgBuiltInType, ScalarConstExpr, SelectStatement, SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        problem::ArrayProblem,
        scalar_expr::SemScalarExprNode,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// ARRAY(query) with its analyzed child and array-result domain.
/// Successful construction is non-null even for zero rows or NULL elements.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArraySubqueryExpr {
    query: Box<SelectStatement>,
    types: InputTypeConstraint,
    invalid_shape: bool,
    #[serde(skip)]
    node: Option<usize>,
}

impl ArraySubqueryExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        query: &SelectStatement,
    ) -> Result<Self, AnalysisError> {
        let mut value = Self {
            query: Box::new(query.clone()),
            types: InputTypeConstraint::any(),
            invalid_shape: false,
            node: None,
        };
        value.query.walk(&mut UnknownOutput { context, depth: 0 })?;
        value.validate_shape(context)?;
        value.refresh_type();
        Ok(value)
    }
}

#[cfg(test)]
mod test_correlated_binding_and_serialized_query_survive_analysis {
    use crate::{
        ast::Dialect,
        sem::{PgBuiltInType, SemScalarExpr},
        test_helpers::query::{expression as projection, resolve},
    };

    #[test]
    fn correlated_binding_and_serialized_query_survive_analysis() {
        let (query, context) = resolve(
            "SELECT ARRAY(SELECT s.n WHERE s.n IS NOT NULL ORDER BY s.n) FROM (SELECT 1 AS n) s",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let array = projection(&query).as_array_subquery().unwrap();
        let child = projection(array.query());
        assert!(child.constraint_binding().is_some());
        assert_eq!(child.get_type(), Some(PgBuiltInType::int4()));
        let encoded = ::serde_json::to_string(projection(&query)).unwrap();
        let decoded: SemScalarExpr = ::serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.get_type(), projection(&query).get_type());
        assert!(decoded.as_array_subquery().is_some());
    }
}

impl ArraySubqueryExpr {
    /// The analyzed query, including completed name bindings and query clauses.
    #[must_use]
    pub fn query(&self) -> &SelectStatement {
        &self.query
    }

    /// Array-result candidates; unrestricted while unresolved, empty on failure.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }

    fn validate_shape(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        if !self.invalid_shape && !self.query.has_pending_stars() && !self.query.has_width(1) {
            self.invalid_shape = true;
            self.types = InputTypeConstraint::types([]);
            context.report_analysis_problem(AnalysisProblem::array(
                ArrayProblem::subquery_columns(self.query.result_columns().len()),
            ))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_invalid_output_shape_retains_query_and_empty_domain {
    use crate::test_helpers::query_array_subquery::reports_width_problem;

    use crate::{
        ast::Dialect,
        parse_statements,
        sem::{PgBuiltInType, SemAst},
        test_helpers::{
            create_cache_context,
            query::{catalog, expression as projection, resolve},
        },
    };

    #[rstest::rstest]
    #[case::zero("SELECT")]
    #[case::two("SELECT 1,2")]
    #[case::star("SELECT s.* FROM (SELECT 1 AS n, 2 AS m) s")]
    fn invalid_output_shape_retains_query_and_empty_domain(
        #[case] child: &str,
        #[values(false, true)] infer: bool,
    ) {
        let (query, _) = resolve(
            &format!("SELECT ARRAY({child})"),
            Dialect::postgres(),
            infer,
        )
        .unwrap();
        let array = projection(&query).as_array_subquery().unwrap();
        assert_ne!(array.query().result_columns().len(), 1);
        assert_eq!(array.types().admissible_types(), Some(&[][..]));
    }

    #[test]
    fn invalid_array_query_does_not_stop_following_statement() {
        let mut cache_context = create_cache_context();
        let mut context = catalog();
        let mut results = Vec::new();
        for statement in parse_statements(
            "SELECT ARRAY(SELECT 1,2); SELECT ARRAY(SELECT 1)",
            Dialect::postgres(),
        ) {
            let analyzed =
                SemAst::from_ast(&statement.unwrap(), context, &mut cache_context).unwrap();
            results.push(analyzed.ast().clone().into_select_statemtnt().unwrap());
            context = analyzed.into_context();
        }
        assert!(reports_width_problem(context.reported_problems()));
        assert_eq!(projection(&results[0]).get_type(), None);
        assert_eq!(
            projection(&results[1]).get_type(),
            Some(PgBuiltInType::int4().array_type())
        );
    }
}

impl ArraySubqueryExpr {
    fn refresh_type(&mut self) {
        if self.node.is_none() && !self.invalid_shape && !self.query.has_pending_stars() {
            self.types = self
                .query
                .result_columns()
                .scalar_type()
                .map(|ty| InputTypeConstraint::exact(&ty.array_type()))
                .unwrap_or_else(InputTypeConstraint::any);
        }
    }
}

#[cfg(test)]
mod test_array_query_retains_child_type_domain_identity {
    use crate::{
        ast::Dialect,
        test_helpers::query::{expression as projection, resolve},
    };

    #[rstest::rstest]
    #[case::one("SELECT 1")]
    #[case::empty("SELECT 1 WHERE false")]
    #[case::many("SELECT 1 UNION ALL SELECT 2")]
    #[case::null("SELECT NULL::integer")]
    #[case::unknown_null("SELECT NULL")]
    #[case::unknown_string("SELECT 'hello'")]
    #[case::array("SELECT ARRAY[1,2]")]
    #[case::empty_array_result("SELECT NULL::integer[] WHERE false")]
    #[case::nested("SELECT ARRAY(SELECT 1)")]
    #[case::star("SELECT s.* FROM (SELECT 1 AS n) s")]
    #[case::clauses("SELECT 1 AS n ORDER BY n LIMIT 2 OFFSET 0")]
    fn array_query_retains_child_type_domain_identity(
        #[case] child: &str,
        #[values(false, true)] infer: bool,
    ) {
        let (query, context) = resolve(
            &format!("SELECT ARRAY({child}) AS result"),
            Dialect::postgres(),
            infer,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let columns = query.result_columns();
        let column = columns.get_column_at(0).unwrap();
        let array = column.expression().unwrap().as_array_subquery().unwrap();
        assert_eq!(array.query().result_columns().len(), 1);
        assert_eq!(
            array.types().single_type().as_ref(),
            column.col_type().as_ref()
        );
    }

    #[test]
    fn anonymous_record_identity_is_retained_in_array_type() {
        let (query, context) =
            resolve("SELECT ARRAY(SELECT ROW(1,'x'))", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let array = projection(&query).get_type().unwrap();
        assert_eq!(array.is_array(), Some(true));
        assert!(array.element_type().type_definition().is_some());
    }
}

impl ArraySubqueryExpr {
    pub(crate) fn expand_pending_stars(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let changed = self.query.expand_pending_stars(context)?;
        self.validate_shape(context)?;
        Ok(changed)
    }
}

#[cfg(test)]
mod test_deferred_star_checks_final_output_width {
    use crate::test_helpers::{
        query::expression as projection, query_array_subquery as test_helpers,
    };

    #[rstest::rstest]
    #[case::one("singleton", true)]
    #[case::two("pair", false)]
    fn deferred_star_checks_final_output_width(#[case] name: &str, #[case] valid: bool) {
        let analyzed = test_helpers::composite_query(&format!(
            "SELECT ARRAY(SELECT ($1).*) AS result, take_{name}($1)"
        ));
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let array = projection(&query).as_array_subquery().unwrap();
        if !valid {
            assert_eq!(array.types().admissible_types(), Some(&[][..]));
        }
        assert_eq!(
            array.query().result_columns().len(),
            if valid { 1 } else { 2 }
        );
    }
}

impl ArraySubqueryExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.query.walk(visitor)?;
        visitor.array_subquery(self)?;
        self.refresh_type();
        Ok(())
    }
}

#[cfg(test)]
mod test_child_parameters_are_visited_and_finalized {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::{AnalysisError, PgBuiltInType, SemScalarExpr, TypeReference},
        test_helpers::query::resolve,
    };

    use std::collections::BTreeMap;

    struct Parameters(Vec<Option<TypeReference>>);

    impl crate::sem::traversal::SemanticVisitorMut for Parameters {
        fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if matches!(value, SemScalarExpr::Param(_)) {
                self.0.push(value.get_type());
            }
            Ok(())
        }
    }

    #[rstest::rstest]
    #[case::projection("SELECT ARRAY(SELECT $1::integer)")]
    #[case::predicate("SELECT ARRAY(SELECT 1 WHERE $1::integer IS NULL)")]
    #[case::order("SELECT ARRAY(SELECT 1 ORDER BY $1::integer)")]
    #[case::correlated("SELECT ARRAY(SELECT s.n WHERE $1::integer IS NULL) FROM (SELECT 1 AS n) s")]
    #[case::nested("SELECT ARRAY(SELECT ARRAY(SELECT $1::integer))")]
    #[case::repeated("SELECT ARRAY(SELECT $1::integer), $1::integer")]
    fn child_parameters_are_visited_and_finalized(#[case] sql: &str) {
        let (mut query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        let mut parameters = Parameters(Vec::new());
        query.walk(&mut parameters).unwrap();
        assert!(!parameters.0.is_empty());
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int2())])));
        // Both smallint and integer inputs can satisfy the cast, so the parameter
        // has no single inferred type even after its constraints are finalized.
        assert!(parameters.0.into_iter().all(|ty| ty.is_none()));
    }
}

impl ArraySubqueryExpr {
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.query.visit(visitor)?;
        visitor.array_subquery(self)?;
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.validate_shape(context)?;
        let single_output = !self.invalid_shape && !self.query.has_pending_stars();
        let fallback = if self.invalid_shape {
            InputTypeConstraint::types([])
        } else if single_output {
            self.query
                .result_columns()
                .scalar_type()
                .map(|ty| InputTypeConstraint::exact(&ty.array_type()))
                .unwrap_or_else(InputTypeConstraint::any)
        } else {
            InputTypeConstraint::any()
        };
        let (node, types) = parameters.array_query_result(&fallback, single_output, self.node);
        self.node = Some(node);
        self.types = types;
        Ok(())
    }
}

impl ArraySubqueryExpr {
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

impl SemScalarExprNode for ArraySubqueryExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.types.admissible_types() == Some(&[]) || self.query.has_pending_stars() {
            None
        } else {
            Some(true)
        }
    }
}

// PostgreSQL resolves an untyped NULL projection to text at the query boundary.
// Limit this normalization to this ARRAY query; nested query analysis keeps its
// own output/coercion rules and unresolved parameters retain their domains.
struct UnknownOutput<'a, C: ParseContext> {
    context: &'a C,
    depth: usize,
}

impl<C: ParseContext> crate::sem::traversal::SemanticVisitorMut for UnknownOutput<'_, C> {
    fn enter_query(&mut self) {
        self.depth += 1;
    }
    fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn output(&mut self, _name: &str, value: &mut SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres()
            && self.depth == 1
            && value.as_const().is_some_and(ScalarConstExpr::is_null)
        {
            *value = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::text(), value);
        }
    }
}
