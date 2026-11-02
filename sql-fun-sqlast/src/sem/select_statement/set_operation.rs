use crate::{
    ast::Dialect,
    sem::{
        AnalysisError, AnalysisProblem, ConstrainedExpr, InputTypeConstraint, MinMaxExpr,
        ParameterConstraints, ParseContext, QueryProblem, ResultColumnCollection, SelectStatement,
        SemScalarExpr, TypeReference, parameter_resolution::ParameterResolution,
        traversal::SemanticVisitorMut,
    },
};

/// The SQL operation combining two query results.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SetOperationKind {
    /// Rows from either operand.
    Union,
    /// Rows present in both operands.
    Intersect,
    /// Left rows absent from the right operand.
    Except,
}

impl SetOperationKind {
    /// Construct UNION semantics.
    pub const fn union() -> Self {
        Self::Union
    }
    /// Construct INTERSECT semantics.
    pub const fn intersect() -> Self {
        Self::Intersect
    }
    /// Construct EXCEPT semantics.
    pub const fn except() -> Self {
        Self::Except
    }
    const fn name(self) -> &'static str {
        match self {
            Self::Union => "UNION",
            Self::Intersect => "INTERSECT",
            Self::Except => "EXCEPT",
        }
    }
    const fn not_null(self, left: Option<bool>, right: Option<bool>) -> Option<bool> {
        match self {
            Self::Union => match (left, right) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            Self::Intersect => match (left, right) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
            Self::Except => left,
        }
    }
}

#[cfg(test)]
mod test_set_nullability {
    use super::SetOperationKind;

    #[rstest::rstest]
    #[case(SetOperationKind::union(), Some(true), Some(true), Some(true))]
    #[case(SetOperationKind::union(), Some(true), None, None)]
    #[case(SetOperationKind::union(), None, Some(false), Some(false))]
    #[case(SetOperationKind::intersect(), None, Some(true), Some(true))]
    #[case(SetOperationKind::intersect(), Some(false), None, None)]
    #[case(SetOperationKind::intersect(), Some(false), Some(false), Some(false))]
    #[case(SetOperationKind::except(), None, Some(true), None)]
    #[case(SetOperationKind::except(), Some(true), Some(false), Some(true))]
    fn combines_proven_facts(
        #[case] operation: SetOperationKind,
        #[case] left: Option<bool>,
        #[case] right: Option<bool>,
        #[case] expected: Option<bool>,
    ) {
        assert_eq!(operation.not_null(left, right), expected);
    }
}

/// A binary operation's original queries and its independent analyzed outputs.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SetOperationQuery {
    kind: SetOperationKind,
    left: SelectStatement,
    right: SelectStatement,
    columns: ResultColumnCollection,
    results: Vec<SetColumnResult>,
    width_invalid: bool,
}

/// Common-type outcome and conversions at one positional output boundary.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct SetColumnResult {
    types: InputTypeConstraint,
    coercions: [Option<TypeReference>; 2],
    failure: Option<SetColumnFailure>,
    #[serde(skip)]
    node: Option<usize>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum SetColumnFailure {
    Incompatible,
    MissingFacts,
}

impl SetColumnResult {
    const fn pending() -> Self {
        Self {
            types: InputTypeConstraint::any(),
            coercions: [None, None],
            failure: None,
            node: None,
        }
    }
    fn coerce(&mut self, inputs: &[SemScalarExpr; 2]) {
        let target = self.types.single_type();
        for (coercion, input) in self.coercions.iter_mut().zip(inputs) {
            *coercion = if MinMaxExpr::input_type(input) != target {
                target.clone()
            } else {
                None
            };
        }
    }
}

impl SetOperationQuery {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        kind: SetOperationKind,
        left: SelectStatement,
        right: SelectStatement,
    ) -> Result<Self, AnalysisError> {
        let mut query = Self {
            kind,
            left,
            right,
            columns: ResultColumnCollection::default(),
            results: Vec::new(),
            width_invalid: false,
        };
        query.resolve_outputs(context)?;
        Ok(query)
    }

    /// Operation identity, independently of its ALL modifier on the query boundary.
    pub const fn kind(&self) -> SetOperationKind {
        self.kind
    }
    /// Original analyzed left operand.
    pub const fn left(&self) -> &SelectStatement {
        &self.left
    }
    /// Original analyzed right operand.
    pub const fn right(&self) -> &SelectStatement {
        &self.right
    }
    /// Combined output metadata in SQL column order.
    pub const fn columns(&self) -> &ResultColumnCollection {
        &self.columns
    }
    /// Whether analysis has established an invalid output shape or column type.
    pub fn is_invalid(&self) -> bool {
        self.width_invalid
            || self
                .results
                .iter()
                .any(|result| result.failure == Some(SetColumnFailure::Incompatible))
    }
    pub(crate) fn can_apply(&self) -> bool {
        !self.is_invalid()
            && !self.has_pending_stars()
            && self.results.iter().all(|result| result.failure.is_none())
    }
    /// Admissible common types of a positional output, including unresolved/empty domains.
    pub fn output_types(&self, index: usize) -> Option<&InputTypeConstraint> {
        self.results.get(index).map(|result| &result.types)
    }
    /// Required implicit coercions for the left and right column respectively.
    pub fn output_coercions(&self, index: usize) -> Option<&[Option<TypeReference>; 2]> {
        self.results.get(index).map(|result| &result.coercions)
    }

    pub(crate) fn has_pending_stars(&self) -> bool {
        self.left.has_pending_stars() || self.right.has_pending_stars()
    }
    pub(crate) const fn has_width(&self, width: usize) -> bool {
        !self.width_invalid && self.columns.len() == width
    }
    pub(crate) fn rename_columns(&mut self, names: &[String]) {
        self.columns.rename(names);
    }
    pub(crate) fn expand_pending_stars(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let left = self.left.expand_pending_stars(context)?;
        let right = self.right.expand_pending_stars(context)?;
        if left || right {
            self.resolve_outputs(context)?;
        }
        Ok(left || right)
    }

    fn inputs(&self, index: usize) -> Option<[SemScalarExpr; 2]> {
        Some([
            self.left.result_columns().set_operand(index)?,
            self.right.result_columns().set_operand(index)?,
        ])
    }

    fn resolve_outputs(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        let pending = self.has_pending_stars();
        let left = self.left.result_columns();
        let right = self.right.result_columns();
        if !pending && left.len() != right.len() && !self.width_invalid {
            self.width_invalid = true;
            context.report_analysis_problem(AnalysisProblem::query(QueryProblem::set_width(
                self.kind.name(),
                left.len(),
                right.len(),
            )))?;
        }
        self.columns.retain_set_names(left);
        self.results
            .resize_with(left.len(), SetColumnResult::pending);
        if pending {
            return Ok(());
        }
        for index in 0..self.results.len() {
            let Some(inputs) = self.inputs(index) else {
                self.results[index].types = InputTypeConstraint::types([]);
                self.results[index].failure = Some(SetColumnFailure::Incompatible);
                self.columns
                    .set_output(index, &self.results[index].types, None, None);
                continue;
            };
            self.resolve_column(context, index, &inputs)?;
        }
        Ok(())
    }

    fn resolve_column(
        &mut self,
        context: &mut impl ParseContext,
        index: usize,
        inputs: &[SemScalarExpr; 2],
    ) -> Result<(), AnalysisError> {
        let result = &mut self.results[index];
        if context.dialect() == Dialect::sqlite() {
            // SQLite does not apply PostgreSQL common-type coercions. Retain
            // its existing left output type, while still checking the width.
            result.types = inputs[0]
                .get_type()
                .as_ref()
                .map(InputTypeConstraint::exact)
                .unwrap_or_else(InputTypeConstraint::any);
        } else if result.failure.is_none()
            && let [Some(left), Some(right)] = inputs.each_ref().map(MinMaxExpr::input_type)
        {
            match MinMaxExpr::select_common_type(context, &[left, right]) {
                Ok(target) => {
                    result.types = InputTypeConstraint::exact(&target);
                    result.coerce(inputs);
                }
                Err(problem) => {
                    if matches!(
                        problem,
                        crate::sem::problem::MinMaxProblem::Incompatible(..)
                    ) {
                        result.failure = Some(SetColumnFailure::Incompatible);
                        result.types = InputTypeConstraint::types([]);
                    } else {
                        result.failure = Some(SetColumnFailure::MissingFacts);
                        result.types = InputTypeConstraint::any();
                    }
                    context.report_analysis_problem(AnalysisProblem::query(
                        QueryProblem::set_type(self.kind.name(), index + 1, &problem),
                    ))?;
                }
            }
        }
        self.columns.set_output(
            index,
            &result.types,
            self.kind
                .not_null(inputs[0].is_not_null(), inputs[1].is_not_null()),
            None,
        );
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.expand_pending_stars(context)?;
        self.resolve_outputs(context)?;
        if self.has_pending_stars() {
            return Ok(());
        }
        for index in 0..self.results.len() {
            self.collect_output(context, parameters, index)?;
        }
        Ok(())
    }

    fn collect_output(
        &mut self,
        context: &mut impl ParseContext,
        parameters: &mut ParameterResolution<'_>,
        index: usize,
    ) -> Result<(), AnalysisError> {
        let inputs = self.inputs(index);
        let result = &mut self.results[index];
        let (node, types) = if result.failure.is_none()
            && context.dialect() == Dialect::postgres()
            && let Some(inputs) = &inputs
        {
            parameters.unify_set_types(context, self.kind.name(), inputs)?
        } else {
            let value = parameters.operation(self.kind.name(), &[], &result.types, None);
            (
                value.constraint_node().expect("set output node"),
                result.types.clone(),
            )
        };
        let not_null = if let Some(inputs) = &inputs {
            self.kind
                .not_null(inputs[0].is_not_null(), inputs[1].is_not_null())
        } else {
            None
        };
        // Preserve published identities across star expansion and resumed solves.
        let node = parameters.retain_output_node(result.node, node);
        result.node = Some(node);
        result.types = types;
        let expression = SemScalarExpr::constrained(&ConstrainedExpr::new(
            node,
            self.kind.name(),
            &[],
            &result.types,
            not_null,
            "",
        ));
        self.columns
            .set_output(index, &result.types, not_null, Some(expression));
        Ok(())
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        for index in 0..self.results.len() {
            let Some(inputs) = self.inputs(index) else {
                continue;
            };
            let result = &mut self.results[index];
            if let Some(node) = result.node {
                result.types = constraints.node_domain(node).clone();
                if result.types.admissible_types() == Some(&[]) {
                    result.failure = Some(SetColumnFailure::Incompatible);
                }
                result.coerce(&inputs);
            }
            self.columns.finalize_set_output(
                index,
                constraints,
                self.kind
                    .not_null(inputs[0].is_not_null(), inputs[1].is_not_null()),
            );
        }
    }

    pub(crate) fn assign_update_output<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        index: usize,
        ty: &TypeReference,
    ) -> Result<(), AnalysisError> {
        self.columns.assign_update_output(visitor, index, ty)
    }
    pub(crate) fn walk_assigned<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        targets: &[crate::sem::InsertTarget],
    ) -> Result<(), AnalysisError> {
        visitor.enter_set_operand();
        self.left.walk(visitor)?;
        visitor.leave_set_operand();
        visitor.enter_set_operand();
        self.right.walk(visitor)?;
        visitor.leave_set_operand();
        visitor.set_operation(self)?;
        self.columns.walk_set_outputs(visitor, targets)?;
        Ok(())
    }
    pub(crate) fn visit_assigned<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
        targets: &[crate::sem::InsertTarget],
    ) -> Result<(), AnalysisError> {
        visitor.access_purpose(crate::sem::AccessKind::select_output());
        visitor.enter_set_operation();
        visitor.enter_set_operand();
        self.left.visit(visitor)?;
        visitor.leave_set_operand();
        visitor.enter_set_operand();
        self.right.visit(visitor)?;
        visitor.leave_set_operand();
        visitor.set_operation(self)?;
        self.columns.visit_set_outputs(visitor, targets)?;
        visitor.leave_set_operation();
        Ok(())
    }
}

#[cfg(test)]
mod test_set_operation {
    use super::{SetOperationKind, SetOperationQuery};
    use crate::{
        sem::{ParseContext, PgBuiltInType, SelectStatement},
        test_helpers::set_operations,
    };
    use std::collections::BTreeMap;

    #[test]
    fn common_output_retains_original_children_and_serialized_coercions() -> testresult::TestResult
    {
        let analyzed = set_operations::analyze("SELECT 1 AS n UNION SELECT 2::bigint AS ignored")?;
        assert!(
            analyzed.context().problems().is_empty(),
            "{:?}",
            analyzed.context().problems()
        );
        let query = analyzed.ast().select_statement().unwrap();
        let SelectStatement::Set(set) = query.query_clauses().unwrap().body() else {
            panic!("set");
        };
        assert_eq!(set.kind(), SetOperationKind::union());
        assert_eq!(
            set.left()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::int4())
        );
        assert_eq!(
            set.right()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::int8())
        );
        assert_eq!(
            set.output_coercions(0),
            Some(&[Some(PgBuiltInType::int8()), None])
        );
        let encoded = serde_json::to_string(set)?;
        let restored: SetOperationQuery = serde_json::from_str(&encoded)?;
        assert_eq!(
            restored.columns().composite_field_collection()?,
            set.columns().composite_field_collection()?
        );
        assert_eq!(restored.output_coercions(0), set.output_coercions(0));
        assert!(!restored.is_invalid());
        assert_eq!(
            query.returning_composite_type()?,
            query
                .query_clauses()
                .unwrap()
                .body()
                .returning_composite_type()?
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case("SELECT ($1).* WHERE take_pair($1) IS NULL UNION SELECT 1,2", false)]
    #[case("SELECT ($1).* WHERE take_pair($1) IS NULL UNION SELECT 1", true)]
    fn pending_stars_finish_before_width_validation(
        #[case] sql: &str,
        #[case] invalid: bool,
    ) -> testresult::TestResult {
        let analyzed = crate::parse_statements(sql, crate::ast::Dialect::postgres())
            .next()
            .unwrap()?
            .analyze(
                set_operations::composite_context()?,
                &mut crate::test_helpers::create_cache_context(),
            )?;
        let query = analyzed.ast().select_statement().unwrap();
        let SelectStatement::Set(set) = query.query_clauses().unwrap().body() else {
            panic!("set");
        };
        assert_eq!(set.columns().len(), 2);
        assert_eq!(set.is_invalid(), invalid);
        assert!(!set.has_pending_stars());
        assert_eq!(
            analyzed
                .context()
                .problems()
                .iter()
                .filter(|problem| problem.code() == "query_set_column_count")
                .count(),
            usize::from(invalid)
        );
        assert!(
            analyzed
                .context()
                .parameter_constraints()
                .result_column(1)
                .is_some()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case("SELECT 1 UNION SELECT 'x'::text")]
    #[case("SELECT 1 INTERSECT SELECT 1,2")]
    fn failures_retain_operands_and_explicit_invalid_state(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let analyzed = set_operations::analyze(sql)?;
        let query = analyzed.ast().select_statement().unwrap();
        let SelectStatement::Set(set) = query.query_clauses().unwrap().body() else {
            panic!("set");
        };
        assert!(set.is_invalid());
        assert!(!set.left().result_columns().is_empty());
        assert!(!set.right().result_columns().is_empty());
        assert_eq!(analyzed.context().problems().len(), 1);
        let restored: SetOperationQuery = serde_json::from_str(&serde_json::to_string(set)?)?;
        assert!(restored.is_invalid());
        Ok(())
    }

    #[rstest::rstest]
    #[case("SELECT $1 AS n UNION SELECT 1")]
    #[case("SELECT n FROM (SELECT $1 AS n UNION SELECT 1) q")]
    #[case("WITH q AS (SELECT $1 AS n UNION SELECT 1) SELECT n FROM q")]
    #[case("SELECT 1 AS n UNION SELECT $1 UNION SELECT 2")]
    fn output_domains_follow_both_operands_and_nested_bindings(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let analyzed = set_operations::analyze(sql)?;
        assert!(
            analyzed.context().problems().is_empty(),
            "{:?}",
            analyzed.context().problems()
        );
        let constraints = analyzed.context().parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        let output = constraints.result_column(0).unwrap();
        assert!(
            output
                .admissible_types()
                .unwrap()
                .contains(&PgBuiltInType::int4())
        );
        assert!(
            output
                .admissible_types()
                .unwrap()
                .contains(&PgBuiltInType::int8())
        );
        assert!(constraints.result_column(1).is_none());
        Ok(())
    }
    #[test]
    fn operand_assignments_remain_correlated() -> testresult::TestResult {
        let analyzed = set_operations::analyze("SELECT $1 AS x UNION SELECT $2")?;
        let constraints = analyzed.context().parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::int8())
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::text())
        ])));
        assert!(constraints.result_column(0).is_some());
        assert!(constraints.result_column(1).is_none());
        Ok(())
    }

    #[rstest::rstest]
    #[case("SELECT $1 AS id UNION SELECT x.id FROM table_x AS x")]
    #[case("SELECT x.id FROM table_x AS x UNION SELECT $1")]
    fn catalog_column_constrains_both_input_and_output_domains(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let catalog = crate::test_helpers::ownership::apply_ownership_sql(
            set_operations::context(crate::ast::Dialect::postgres()),
            "CREATE TABLE table_x(id integer)",
        )?;
        let analyzed = crate::parse_statements(sql, crate::ast::Dialect::postgres())
            .next()
            .unwrap()?
            .analyze(
                catalog.into_context(),
                &mut crate::test_helpers::create_cache_context(),
            )?;
        assert!(analyzed.context().problems().is_empty());
        let constraints = analyzed.context().parameter_constraints();
        let input = constraints.parameter(1).unwrap().constraint();
        let output = constraints.result_column(0).unwrap();
        for ty in [PgBuiltInType::int4(), PgBuiltInType::int8()] {
            assert!(input.allows(&ty));
            assert!(constraints.allows(&BTreeMap::from([(1, ty.clone())])));
            assert!(output.allows(&ty));
        }
        assert!(!input.allows(&PgBuiltInType::text()));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        assert!(!output.allows(&PgBuiltInType::text()));
        assert!(
            analyzed
                .ast()
                .select_statement()
                .unwrap()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type()
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn array_query_consumes_the_combined_output_domain() -> testresult::TestResult {
        let analyzed = set_operations::analyze("SELECT ARRAY(SELECT $1 UNION SELECT 1) AS values")?;
        let output = analyzed
            .context()
            .parameter_constraints()
            .result_column(0)
            .unwrap();
        assert!(output.allows(&PgBuiltInType::int4().array_type()));
        assert!(output.allows(&PgBuiltInType::int8().array_type()));
        assert!(!output.allows(&PgBuiltInType::text().array_type()));
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn heterogeneous_operands_do_not_require_postgres_common_types() -> testresult::TestResult {
        use crate::ast::Dialect;
        let sql = "SELECT 1 AS x UNION SELECT 'text'";
        let oracle = rusqlite::Connection::open_in_memory()?;
        let count: i64 = oracle.query_row(
            "SELECT count(*) FROM (SELECT 1 AS x UNION SELECT 'text')",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 2);
        let analyzed = crate::parse_statements(sql, Dialect::sqlite())
            .next()
            .unwrap()?
            .analyze(
                set_operations::context(Dialect::sqlite()),
                &mut crate::test_helpers::create_cache_context(),
            )?;
        assert!(
            analyzed.context().problems().is_empty(),
            "{:?}",
            analyzed.context().problems()
        );
        Ok(())
    }
}
