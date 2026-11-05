//! Parameter inference over the name-resolved semantic tree.

#[cfg(test)]
mod case_tests;
#[cfg(test)]
mod indirection_tests;

use crate::{
    CacheContext,
    names::{ColumnName, OperatorName},
    sem::{
        AliasSpec, AnalysisProblem, ArraySubqueryExpr, ColumnCollection, ColumnDefinition,
        ColumnReferenceExpr, DeleteStatement, InsertStatement, InsertTarget, MembershipExpr,
        MergeStatement, MinMaxExpr, ParseContext, PgBuiltInType, QueryName, QueryOrder,
        QueryWindow, ResolvedOperator, ResultColumnCollection, SelectStatement, SetOperationQuery,
        TableSampleSource, TruthTestExpr, UpdateStatement, parameter_constraints::TypeProjection,
        problem::MinMaxProblem,
    },
};

use crate::{
    ast::Dialect,
    context::BaseParseContext,
    sem::{
        AnalysisError, ConstrainedExpr, ConstraintGraph, InputTypeConstraint, ParameterConstraints,
        SemScalarExpr, SignatureAlternative, TypeReference,
        overload_rank::OverloadRank,
        type_system::{ArgumentBindingCollection, SimpleInferResult},
    },
};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
struct Outputs(Vec<Output>);

impl Outputs {
    fn constraints(&self, graph: &ConstraintGraph) -> Vec<InputTypeConstraint> {
        self.0
            .iter()
            .map(|output| output.constraint(graph).clone())
            .collect()
    }

    fn renamed(&self, columns: &[String]) -> Self {
        let mut outputs = self.clone();
        for (output, name) in outputs.0.iter_mut().zip(columns) {
            output.rename(name);
        }
        outputs
    }

    const fn scalar_node(&self) -> Option<usize> {
        match self.0.as_slice() {
            [output] => output.node,
            _ => None,
        }
    }

    fn column(&self, original: &SemScalarExpr, graph: &ConstraintGraph) -> Option<SemScalarExpr> {
        let (_, column) = original.constraint_binding()?;
        self.0
            .iter()
            .find(|output| output.has_name(column.as_str()))?
            .column(original, graph)
    }

    fn push(&mut self, name: &str, value: &SemScalarExpr, text_projection: bool) {
        self.0.push(Output::new(name, value, text_projection));
    }
}

#[derive(Clone)]
struct Output {
    name: String,
    node: Option<usize>,
    domain: InputTypeConstraint,
}

impl Output {
    fn new(name: &str, value: &SemScalarExpr, text_projection: bool) -> Self {
        let domain = if text_projection {
            InputTypeConstraint::exact(&PgBuiltInType::text())
        } else {
            value
                .get_type()
                .as_ref()
                .map(InputTypeConstraint::exact)
                .unwrap_or_else(InputTypeConstraint::any)
        };
        Self {
            name: name.to_owned(),
            node: value.constraint_node(),
            domain,
        }
    }
}

#[cfg(test)]
mod test_plain_projection_keeps_any_input_and_postgres_text_output {
    use crate::{
        ast::Dialect, context::ParseContext, sem::PgBuiltInType,
        test_helpers::public_analysis::analyze,
    };

    #[rstest::rstest]
    #[case::direct("select $1")]
    #[case::cte("with x as (select $1 as n) select n from x")]
    fn plain_projection_keeps_any_input_and_postgres_text_output(#[case] sql: &str) {
        let result = analyze(sql, Dialect::postgres()).unwrap();
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
        assert_eq!(
            result
                .context()
                .parameter_constraints()
                .result_column(0)
                .unwrap()
                .admissible_types(),
            Some([PgBuiltInType::text()].as_slice())
        );
        let select = result.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            select.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::text())
        );
    }
}

impl Output {
    fn constraint<'a>(&'a self, graph: &'a ConstraintGraph) -> &'a InputTypeConstraint {
        if let Some(node) = self.node {
            let domain = graph.domain(node);
            if !domain.is_any() {
                return domain;
            }
        }
        &self.domain
    }

    fn rename(&mut self, name: &str) {
        self.name = name.to_owned();
    }

    fn has_name(&self, name: &str) -> bool {
        self.name == name
    }

    fn column(&self, original: &SemScalarExpr, graph: &ConstraintGraph) -> Option<SemScalarExpr> {
        Some(SemScalarExpr::constrained(&ConstrainedExpr::new(
            self.node?,
            "column",
            std::slice::from_ref(original),
            self.constraint(graph),
            original.is_not_null(),
            &self.name,
        )))
    }
}

#[derive(Clone, Default)]
struct QueryScope {
    ctes: BTreeMap<String, Outputs>,
    sources: BTreeMap<String, Outputs>,
    outputs: Outputs,
    window_orders: Vec<Option<TypeReference>>,
}

impl QueryScope {
    fn nested(&self) -> Self {
        Self {
            ctes: self.ctes.clone(),
            ..Self::default()
        }
    }

    fn cte(&mut self, name: &str, outputs: &Outputs, columns: &[String]) {
        self.ctes.insert(name.to_owned(), outputs.renamed(columns));
    }

    fn relation(&mut self, name: Option<&str>, alias: &str, columns: &[String]) {
        let outputs = if let Some(name) = name
            && let Some(outputs) = self.ctes.get(name)
        {
            outputs.clone()
        } else {
            Default::default()
        };
        self.derived(alias, &outputs, columns);
    }

    fn derived(&mut self, alias: &str, outputs: &Outputs, columns: &[String]) {
        self.sources
            .insert(alias.to_owned(), outputs.renamed(columns));
    }

    fn has_source(&self, alias: &str) -> bool {
        self.sources.contains_key(alias)
    }

    fn column(&self, original: &SemScalarExpr, graph: &ConstraintGraph) -> Option<SemScalarExpr> {
        let (alias, _) = original.constraint_binding()?;
        self.sources.get(alias)?.column(original, graph)
    }
}

pub(crate) struct ParameterResolution<'cache> {
    cache: &'cache mut CacheContext,
    graph: ConstraintGraph,
    scopes: Vec<QueryScope>,
    completed: Outputs,
}

// Memoized graph combinations belong to this inference, including provisional
// solves, not to the lifetime of the retained immutable catalog base.
impl Drop for ParameterResolution<'_> {
    fn drop(&mut self) {
        self.cache.clear_constraint_memo();
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn unify_set_types(
        &mut self,
        context: &mut impl ParseContext,
        operation: &str,
        inputs: &[SemScalarExpr],
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        self.common_type(context, operation, inputs, false)
    }

    pub(crate) fn retain_output_node(&mut self, previous: Option<usize>, current: usize) -> usize {
        if let Some(previous) = previous {
            self.graph.equal(previous, current);
            previous
        } else {
            current
        }
    }

    pub(crate) fn new(cache: &'cache mut CacheContext) -> Self {
        Self {
            cache,
            graph: ConstraintGraph::default(),
            scopes: Vec::new(),
            completed: Outputs::default(),
        }
    }

    pub(crate) fn constraints(&self) -> ParameterConstraints {
        let mut constraints = ParameterConstraints::from_graph(&self.graph);
        constraints.set_outputs(&self.completed.constraints(&self.graph));
        constraints
    }

    fn solve(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        self.graph.prepare_projections(context, self.cache);
        self.graph
            .report_projection_problems(context, self.cache.constraint_memo())?;
        if let Err(problem) = self.graph.solve_with(self.cache.constraint_memo()) {
            ParseContext::report_analysis_problem(
                context,
                AnalysisProblem::parameter_constraint(problem.message()),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_conflicting_uses_report_recoverable_problem {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::OperatorName,
        parse_statements,
        sem::{BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType, SemAst},
        test_helpers::{
            create_cache_context,
            public_analysis::{analyze, context},
        },
    };

    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::direct("select $1 + 1 as n, lower($1) as s")]
    #[case::cte("with x as (select $1 + 1 as n) select lower($1) as s from x")]
    fn conflicting_uses_report_recoverable_problem(#[case] sql: &str) {
        let analyzed =
            analyze(sql, Dialect::postgres()).expect("analysis continues after a conflict");
        let problems = analyzed.context().reported_problems();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code(), "parameter_constraint");
        let metadata = problems[0].render_metadata().expect("parameter metadata");
        assert!(metadata.message().contains("$1"));
        assert!(
            !analyzed
                .context()
                .parameter_constraints()
                .allows(&BTreeMap::new())
        );
    }

    #[test]
    fn tied_best_overloads_are_not_accepted_as_a_type_range() {
        let mut cache_context = create_cache_context();
        let mut context = context();
        let int = PgBuiltInType::int4();
        let overloads = OperatorDefinition::binary(&[
            BinaryOperatorDefinition::new(&int, &int, &int, true),
            BinaryOperatorDefinition::new(&int, &int, &PgBuiltInType::text(), true),
        ]);
        context.set_get_operator_op_result(
            &OperatorName::from_parts(None, "+"),
            true,
            true,
            Some(&overloads),
        );
        let analyzed = SemAst::from_ast(
            &parse_statements("select $1 + 1", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .expect("ambiguity is recoverable");
        assert_eq!(analyzed.context().reported_problem_count(), 1);
        assert_eq!(
            analyzed.context().reported_problems()[0].code(),
            "parameter_constraint"
        );
    }
}

impl<'cache> ParameterResolution<'cache> {
    fn complete(
        &self,
        context: &mut impl ParseContext,
        constraints: ParameterConstraints,
    ) -> Result<ParameterConstraints, AnalysisError> {
        // RETURNING and projection expansion use provisional solves. Only the
        // completed statement can establish an unsupported inference boundary.
        self.graph.report_inference_problems(context)?;
        Ok(constraints)
    }

    fn default_query_outputs(&mut self, context: &impl ParseContext) {
        self.graph.prepare_projections(context, self.cache);
        // UPDATE/DELETE subqueries resolve unknown projected parameters before
        // surrounding expressions or assignments constrain them. Keep solver failure
        // in the graph for the final diagnostic-producing solve.
        let _ = self.graph.solve_with(self.cache.constraint_memo());
        if let Some(scope) = self.scopes.last() {
            for output in &scope.outputs.0 {
                if let Some(node) = output.node
                    && self.graph.domain(node).is_any()
                    && output.domain == InputTypeConstraint::exact(&PgBuiltInType::text())
                {
                    self.graph.restrict(node, &output.domain);
                }
            }
        }
    }
    pub(crate) fn default_returning_parameter(&mut self, value: &SemScalarExpr) {
        if value.is_unconstrained_parameter()
            && let Some(node) = value.constraint_node()
            && self.graph.domain(node).is_any()
        {
            self.graph
                .restrict(node, &InputTypeConstraint::exact(&PgBuiltInType::text()));
        }
    }
}

#[cfg(test)]
mod test_nested_returning_projection_fixes_its_unknown_parameter_before_outer_casts {

    use crate::{
        ast::Dialect, context::ParseContext, sem::PgBuiltInType,
        test_helpers::delete as test_helpers,
    };

    #[test]
    fn nested_returning_projection_fixes_its_unknown_parameter_before_outer_casts() {
        let mut context = test_helpers::catalog_with_text_integer_cast();
        let statement = test_helpers::resolve(
            "DELETE FROM items RETURNING (SELECT $1) AS label, $1::integer AS n",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraint = context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint();
        assert!(constraint.allows(&PgBuiltInType::text()));
        assert!(!constraint.allows(&PgBuiltInType::int4()));
        assert_eq!(
            statement
                .as_delete()
                .unwrap()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::text())
        );
    }
}

#[cfg(test)]
mod test_returning_parameter_defaults_to_text_before_merge_action_assignment {
    use crate::{
        sem::SemAst,
        test_helpers::{
            create_cache_context, document_merge::catalog, merge_statements::statement,
        },
    };

    #[test]
    fn returning_parameter_defaults_to_text_before_merge_action_assignment() {
        let mut cache_context = create_cache_context();
        let source = statement(
            "MERGE INTO items AS t USING incoming AS s ON t.id = $2 WHEN MATCHED AND $3 THEN UPDATE SET id = $1 WHEN NOT MATCHED THEN INSERT (id) VALUES ($1) RETURNING $1 AS value",
        );
        let result = SemAst::from_ast(&source, catalog(17), &mut cache_context);
        assert!(match result {
            Err(_) => true,
            Ok(analyzed) => analyzed.context().reported_problem_count() > 0,
        });
    }
}

#[cfg(test)]
mod test_postgres_returning_defaults_unknown_parameters_to_text {
    use crate::{
        ast::Dialect, context::ParseContext, sem::PgBuiltInType, test_helpers::update::analyze,
    };

    #[rstest::rstest]
    #[case::returning("UPDATE items SET label=$1 RETURNING $1 n")]
    #[case::cte("WITH u AS (UPDATE items SET id=1 RETURNING $1 n) SELECT n FROM u")]
    fn postgres_returning_defaults_unknown_parameters_to_text(#[case] sql: &str) {
        let (_, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let parameter = context.parameter_constraints().parameter(1).unwrap();
        assert!(parameter.constraint().allows(&PgBuiltInType::text()));
        assert!(!parameter.constraint().allows(&PgBuiltInType::int4()));
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn parameter(
        &mut self,
        position: u32,
        domain: &InputTypeConstraint,
    ) -> SemScalarExpr {
        let node = self.graph.parameter(position);
        if !domain.is_any() {
            self.graph.restrict(node, domain);
        }
        SemScalarExpr::parameter(position, node, self.graph.domain(node))
    }

    pub(crate) fn unify_case_types(
        &mut self,
        context: &mut impl ParseContext,
        results: &[SemScalarExpr],
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        self.common_type(context, "CASE", results, false)
    }

    pub(crate) fn case_condition(
        &mut self,
        context: &impl ParseContext,
        condition: &SemScalarExpr,
    ) -> usize {
        let node = self.input(condition);
        // Bare NULL/string literals have PostgreSQL unknown type and can be
        // coerced to boolean. Parameters instead retain their source domain.
        if MinMaxExpr::input_type(condition) != Some(MinMaxExpr::unknown()) {
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(self.cache.cast_sources(
                    context,
                    &PgBuiltInType::bool(),
                    false,
                )),
            );
        }
        node
    }
    fn predicate_result(
        &mut self,
        comparisons: &[SemScalarExpr],
        require_boolean: bool,
        types: &InputTypeConstraint,
        existing: Option<usize>,
        operation: &str,
    ) -> usize {
        let output = existing.unwrap_or_else(|| self.graph.node(types));
        self.graph.restrict(output, types);
        if require_boolean {
            let boolean = InputTypeConstraint::exact(&PgBuiltInType::bool());
            for comparison in comparisons {
                let input = self.input(comparison);
                self.graph.overload(
                    &[input, output],
                    &[SignatureAlternative::new(&[
                        boolean.clone(),
                        boolean.clone(),
                    ])],
                    operation,
                );
            }
        }
        output
    }
    pub(crate) fn membership_result(
        &mut self,
        comparisons: &[SemScalarExpr],
        require_boolean: bool,
        types: &InputTypeConstraint,
        existing: Option<usize>,
    ) -> usize {
        self.predicate_result(comparisons, require_boolean, types, existing, "membership")
    }
    pub(crate) fn distinct_result(
        &mut self,
        comparisons: &[SemScalarExpr],
        require_boolean: bool,
        types: &InputTypeConstraint,
    ) -> usize {
        self.predicate_result(comparisons, require_boolean, types, None, "distinctness")
    }

    pub(crate) fn truth_test(
        &mut self,
        context: &impl ParseContext,
        argument: &SemScalarExpr,
        types: &InputTypeConstraint,
    ) -> (usize, InputTypeConstraint) {
        let input = if let Some(node) = argument.constraint_node() {
            node
        } else if let Some(ty) = argument.get_type() {
            let canonical = if ty.is_dynamic() {
                ty
            } else {
                context.canonicalize_type_ref(&ty).unwrap_or(ty)
            };
            self.graph.node(&InputTypeConstraint::exact(&canonical))
        } else {
            self.input(argument)
        };
        let output = self.graph.node(types);
        let sources = TruthTestExpr::input_types(context);
        self.graph.restrict(input, &sources);
        self.graph.overload(
            &[input, output],
            &[SignatureAlternative::new(&[sources, types.clone()])],
            "truth test",
        );
        (output, self.graph.domain(output).clone())
    }

    pub(crate) fn unify_coalesce_types(
        &mut self,
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        self.common_type(context, "COALESCE", arguments, false)
    }
    pub(crate) fn row_field(&mut self, field: &SemScalarExpr) -> SemScalarExpr {
        let source = self.input(field);
        let result = self.operation(
            "row field",
            &[],
            &InputTypeConstraint::any(),
            field.is_not_null(),
        );
        self.graph
            .equal(source, result.constraint_node().expect("operation node"));
        result
    }
}

#[cfg(test)]
mod test_parameter_field_remains_connected_to_its_source {
    use crate::{
        ast::Dialect, context::ParseContext, names::ColumnName, sem::PgBuiltInType,
        test_helpers::query::resolve,
    };

    #[rstest::rstest]
    #[case::later("SELECT (ROW($1, 2)).f1 AS value, $1::integer AS hint", 0)]
    #[case::earlier("SELECT $1::integer AS hint, (ROW($1, 2)).f1 AS value", 1)]
    #[case::record_cast("SELECT (ROW($1,2)::record).f1 AS value, $1::integer AS hint", 0)]
    #[case::nested("SELECT ((ROW(2, ROW($1))).f2).f1 AS value, $1::integer AS hint", 0)]
    fn parameter_field_remains_connected_to_its_source(#[case] sql: &str, #[case] output: usize) {
        let (query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(
            query
                .get_result_column_def(&ColumnName::from("value"))
                .unwrap()
                .get_type_cloned(),
            None
        );
        assert_eq!(
            context
                .parameter_constraints()
                .result_column(output)
                .unwrap()
                .admissible_types(),
            Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
        );
        let expression = query
            .result_columns()
            .get_column_with_skip_count(&ColumnName::from("value"), 0)
            .unwrap()
            .expression()
            .unwrap();
        assert_eq!(expression.get_column_name(), "f1");
        assert!(expression.as_indirection().is_some());
        assert!(
            context
                .parameter_constraints()
                .allows(&std::collections::BTreeMap::from([(
                    1,
                    PgBuiltInType::int4()
                )]))
        );
        assert!(
            !context
                .parameter_constraints()
                .allows(&std::collections::BTreeMap::from([(
                    1,
                    PgBuiltInType::bool()
                )]))
        );
    }
}

#[cfg(test)]
mod test_renamed_parameter_fields_retain_constraints {
    use crate::{
        sem::PgBuiltInType,
        test_helpers::{query::expression, query_whole_rows as test_helpers},
    };

    #[rstest::rstest]
    #[case::member("SELECT (r.*).x FROM (SELECT $1::integer AS n) r(x)")]
    #[case::star("SELECT (r.*).* FROM (SELECT $1::integer AS n) r(x)")]
    fn renamed_parameter_fields_retain_constraints(#[case] sql: &str) {
        use crate::context::ParseContext;
        let (query, context) = test_helpers::alias_query(sql);
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let constraint = context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint();
        assert!(constraint.allows(&PgBuiltInType::int4()));
        let (_, control) =
            test_helpers::alias_query("SELECT (r.*).n FROM (SELECT $1::integer AS n) r");
        assert_eq!(
            constraint,
            control
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint(),
        );
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(test_helpers::row_levels(query), vec![0]);
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn member_access(&mut self, base: &SemScalarExpr, member: &str) -> SemScalarExpr {
        let source = self.input(base);
        let result = self.operation(
            "member access",
            &[],
            &InputTypeConstraint::any(),
            base.is_not_null(),
        );
        self.graph.project(
            source,
            result.constraint_node().expect("operation node"),
            TypeProjection::member(member),
        );
        result
    }
    pub(crate) fn array_access(
        &mut self,
        context: &mut impl ParseContext,
        base: &SemScalarExpr,
        bounds: &[&SemScalarExpr],
        slice: bool,
    ) -> SemScalarExpr {
        for bound in bounds {
            let node = self.input(bound);
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(
                    self.cache
                        .assignment_sources(context, &PgBuiltInType::int4()),
                ),
            );
        }
        let source = self.input(base);
        // Scalar reads may select NULL elements or indices outside the array.
        let is_not_null = if slice {
            base.is_not_null()
        } else {
            Some(false)
        };
        let result = self.operation(
            "array access",
            &[],
            &InputTypeConstraint::any(),
            is_not_null,
        );
        let projection = if slice {
            TypeProjection::array()
        } else {
            TypeProjection::array_element()
        };
        self.graph.project(
            source,
            result.constraint_node().expect("operation node"),
            projection,
        );
        result
    }
}

#[cfg(test)]
mod test_synthetic_identity_function_consumes_array_subquery_result {
    use crate::{
        ast::Dialect,
        sem::PgBuiltInType,
        test_helpers::query::{expression as projection, resolve},
    };

    #[test]
    fn synthetic_identity_function_consumes_array_subquery_result() {
        let (query, context) = resolve(
            "SELECT identity((ARRAY(SELECT 1))[1])",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(projection(&query).get_type(), Some(PgBuiltInType::int4()));
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn sqlite_operator(
        &mut self,
        _name: &OperatorName,
        _left: Option<&SemScalarExpr>,
        _right: Option<&SemScalarExpr>,
    ) -> SemScalarExpr {
        todo!("SQLite operator type constraints");
    }
    pub(crate) fn special_operator(&mut self, _operation: &str, _arguments: &[&SemScalarExpr]) {
        todo!("parameter constraints for this operator form");
    }

    fn input(&mut self, value: &SemScalarExpr) -> usize {
        value.constraint_node().unwrap_or_else(|| {
            self.graph.node(
                &value
                    .get_type()
                    .as_ref()
                    .map(InputTypeConstraint::exact)
                    .unwrap_or_else(InputTypeConstraint::any),
            )
        })
    }

    pub(crate) fn operation(
        &mut self,
        operation: &str,
        arguments: &[SemScalarExpr],
        domain: &InputTypeConstraint,
        not_null: Option<bool>,
    ) -> SemScalarExpr {
        let node = self.graph.node(domain);
        SemScalarExpr::constrained(&ConstrainedExpr::new(
            node,
            operation,
            arguments,
            self.graph.domain(node),
            not_null,
            "",
        ))
    }

    fn require_assignment(
        &mut self,
        context: &impl BaseParseContext,
        value: &SemScalarExpr,
        target: &TypeReference,
    ) {
        if let Some(node) = value.constraint_node() {
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(context.assignment_sources(target)),
            );
        }
    }

    pub(crate) fn require(
        &mut self,
        catalog: &impl BaseParseContext,
        value: &SemScalarExpr,
        target: &TypeReference,
        explicit: bool,
    ) {
        if let Some(node) = value.constraint_node() {
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(self.cache.cast_sources(catalog, target, explicit)),
            );
        }
    }
}

#[cfg(test)]
mod test_boolean_context_constrains_input {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::{PgBuiltInType, SemAst},
        test_helpers::{TestParseContext, create_cache_context},
    };

    #[rstest::rstest]
    #[case::boolean("select $1 as flag where $1")]
    #[case::not("select not $1 as flag")]
    fn boolean_context_constrains_input(#[case] sql: &str) {
        let mut cache_context = create_cache_context();
        let result = SemAst::from_ast(
            &parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            TestParseContext::default(),
            &mut cache_context,
        )
        .unwrap();
        let input = result
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint();
        assert!(input.allows(&PgBuiltInType::bool()));
        assert!(!input.allows(&PgBuiltInType::int4()));
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn call(
        &mut self,
        catalog: &impl ParseContext,
        name: &str,
        arguments: &[SemScalarExpr],
        signatures: &[Signature],
    ) -> Result<SemScalarExpr, AnalysisError> {
        self.call_with_diagnostic_name(catalog, name, name, arguments, signatures)
    }

    fn call_with_diagnostic_name(
        &mut self,
        catalog: &impl ParseContext,
        name: &str,
        diagnostic_name: &str,
        arguments: &[SemScalarExpr],
        signatures: &[Signature],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let result = self.operation(name, arguments, &InputTypeConstraint::any(), None);
        let mut nodes = Vec::new();
        for argument in arguments {
            nodes.push(self.input(argument));
        }
        nodes.push(result.constraint_node().expect("operation has node"));
        self.graph
            .call(catalog, self.cache, &nodes, signatures, diagnostic_name);
        Ok(result)
    }

    pub(crate) fn min_max(
        &mut self,
        context: &mut impl ParseContext,
        operation: &str,
        arguments: &[SemScalarExpr],
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        self.common_type(context, operation, arguments, true)
    }
}

#[cfg(test)]
mod test_missing_comparison_knowledge {
    use crate::{
        sem::{InputTypeConstraint, parameter_resolution::ParameterResolution},
        test_helpers::create_cache_context,
    };

    use crate::{
        sem::{PgBuiltInType, ScalarConstExpr, SemScalarExpr},
        test_helpers::TestParseContext,
    };

    #[test]
    fn absent_comparison_knowledge_does_not_eliminate_parameter_types() {
        let mut context = TestParseContext::default();
        let mut cache = create_cache_context();
        let mut parameters = ParameterResolution::new(&mut cache);
        let arguments = [
            parameters.parameter(1, &InputTypeConstraint::any()),
            SemScalarExpr::new_const(ScalarConstExpr::new_integer(1)),
        ];
        let (_, types) = parameters
            .min_max(&mut context, "greatest", &arguments)
            .unwrap();
        assert_eq!(types.single_type(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problems().len(), 1);
        assert_eq!(
            context.reported_problems()[0].code(),
            "min_max_missing_comparison_facts"
        );
    }
}

impl<'cache> ParameterResolution<'cache> {
    // Shared ordered selection relations; comparison validation belongs only
    // to GREATEST/LEAST. ARRAY lifts the selected type into its result below.
    fn common_type(
        &mut self,
        context: &mut impl ParseContext,
        operation: &str,
        arguments: &[SemScalarExpr],
        comparison: bool,
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        use crate::sem::MinMaxExpr;
        let mut choices = std::collections::HashSet::new();
        for argument in arguments {
            if let Some(ty) = MinMaxExpr::input_type(argument)
                && ty != MinMaxExpr::unknown()
            {
                choices.insert(ty.clone());
                choices.extend(context.get_implicit_castable(&MinMaxExpr::base_type(context, &ty)));
            }
            if let Some(node) = argument.constraint_node()
                && let Some(types) = self.graph.domain(node).admissible_types()
            {
                choices.extend(types.iter().cloned());
            }
        }
        if choices.is_empty()
            && arguments
                .iter()
                .all(|argument| argument.constraint_node().is_none())
        {
            let domain = InputTypeConstraint::exact(&PgBuiltInType::text());
            return Ok((self.graph.node(&domain), domain));
        }
        if choices.is_empty() {
            let nodes: Vec<_> = arguments
                .iter()
                .map(|argument| {
                    argument.constraint_node().unwrap_or_else(|| {
                        self.graph
                            .node(&InputTypeConstraint::exact(&MinMaxExpr::unknown()))
                    })
                })
                .collect();
            let node = self
                .graph
                .common_type(context, &nodes, comparison, operation);
            return Ok((node, InputTypeConstraint::any()));
        }
        for target in choices.clone() {
            choices.extend(self.cache.cast_sources(context, &target, false));
        }
        let choices = InputTypeConstraint::types(choices);
        let mut nodes = Vec::new();
        let mut domains = Vec::new();
        for argument in arguments {
            let node = if let Some(node) = argument.constraint_node() {
                self.graph.restrict(node, &choices);
                node
            } else {
                self.graph.node(
                    &MinMaxExpr::input_type(argument)
                        .as_ref()
                        .map(InputTypeConstraint::exact)
                        .unwrap_or_else(InputTypeConstraint::any),
                )
            };
            domains.push(self.graph.domain(node).clone());
            nodes.push(node);
        }
        let mut candidate = nodes[0];
        let mut candidate_types = domains[0].clone();
        // A chain of binary selection relations keeps growth polynomial in the
        // type universe instead of enumerating every argument tuple. Conversion
        // checks relate each original input to the final candidate below.
        for (input, domain) in nodes.iter().zip(&domains).skip(1) {
            let mut alternatives = Vec::new();
            let mut targets = Vec::new();
            for left in candidate_types.admissible_types().unwrap_or(&[]) {
                for right in domain.admissible_types().unwrap_or(&[]) {
                    let selected = if left == right {
                        Ok(left.clone())
                    } else {
                        MinMaxExpr::next_candidate(
                            context,
                            &MinMaxExpr::base_type(context, left),
                            &MinMaxExpr::base_type(context, right),
                        )
                    };
                    if let Ok(target) = selected {
                        alternatives.push(SignatureAlternative::new(&[
                            InputTypeConstraint::exact(left),
                            InputTypeConstraint::exact(right),
                            InputTypeConstraint::exact(&target),
                        ]));
                        targets.push(target);
                    }
                }
            }
            candidate_types = InputTypeConstraint::types(targets);
            let next = self.graph.node(&candidate_types);
            self.graph
                .overload(&[candidate, *input, next], &alternatives, operation);
            candidate = next;
        }
        let mut alternatives = Vec::new();
        let mut targets = Vec::new();
        let mut unordered = None;
        for source in candidate_types.admissible_types().unwrap_or(&[]) {
            let target = if *source == MinMaxExpr::unknown() {
                PgBuiltInType::text()
            } else {
                source.clone()
            };
            let ordering = if comparison {
                MinMaxExpr::ordered(context, &target)
            } else {
                Some(true)
            };
            if ordering.is_none() {
                context.report_analysis_problem(AnalysisProblem::min_max(
                    MinMaxProblem::missing_comparison_facts(&target),
                ))?;
            }
            // Unknown comparison support is not evidence against a type candidate.
            // Retain its type relationship while reporting the missing catalog facts.
            if ordering == Some(false) {
                unordered.get_or_insert(target);
            } else {
                targets.push(target.clone());
                alternatives.push(SignatureAlternative::new(&[
                    InputTypeConstraint::exact(source),
                    InputTypeConstraint::exact(&target),
                ]));
            }
        }
        if targets.is_empty()
            && let Some(target) = unordered
        {
            context.report_analysis_problem(AnalysisProblem::min_max(
                MinMaxProblem::not_ordered(&target),
            ))?;
        }
        let domain = InputTypeConstraint::types(targets);
        let result = self.graph.node(&domain);
        self.graph
            .overload(&[candidate, result], &alternatives, operation);
        for (node, input) in nodes.iter().zip(&domains) {
            let mut alternatives = Vec::new();
            for target in domain.admissible_types().unwrap_or(&[]) {
                let sources = input
                    .admissible_types()
                    .unwrap_or(&[])
                    .iter()
                    .filter(|source| MinMaxExpr::can_coerce(context, source, target))
                    .cloned();
                alternatives.push(SignatureAlternative::new(&[
                    InputTypeConstraint::types(sources),
                    InputTypeConstraint::exact(target),
                ]));
            }
            self.graph
                .overload(&[*node, result], &alternatives, operation);
        }
        Ok((result, domain))
    }

    pub(crate) fn array(
        &mut self,
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
        explicit_target: Option<&TypeReference>,
        multidimensional: bool,
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        if let Some(target) = explicit_target {
            let element = if multidimensional {
                target.clone()
            } else {
                target.element_type()
            };
            for argument in arguments {
                self.require(context, argument, &element, true);
            }
            let domain = InputTypeConstraint::exact(target);
            return Ok((self.graph.node(&domain), domain));
        }
        let (element, domain) = self.common_type(context, "ARRAY", arguments, false)?;
        if domain.is_any() {
            let result = self.graph.node(&InputTypeConstraint::any());
            self.graph
                .project(element, result, TypeProjection::construct_array());
            return Ok((result, InputTypeConstraint::any()));
        }
        let mut alternatives = Vec::new();
        let mut arrays = Vec::new();
        for ty in domain.admissible_types().unwrap_or(&[]) {
            let array = ty.array_type();
            arrays.push(array.clone());
            alternatives.push(SignatureAlternative::new(&[
                InputTypeConstraint::exact(ty),
                InputTypeConstraint::exact(&array),
            ]));
        }
        let domain = InputTypeConstraint::types(arrays);
        let node = self.graph.node(&domain);
        self.graph
            .overload(&[element, node], &alternatives, "ARRAY element/result");
        Ok((node, domain))
    }

    pub(crate) fn membership_common(
        &mut self,
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (node, domain) = self.common_type(context, "IN", arguments, false)?;
        Ok(SemScalarExpr::constrained(&ConstrainedExpr::new(
            node,
            "membership common type",
            arguments,
            &domain,
            None,
            "",
        )))
    }

    pub(crate) fn array_query_result(
        &mut self,
        fallback: &InputTypeConstraint,
        single_output: bool,
        existing: Option<usize>,
    ) -> (usize, InputTypeConstraint) {
        let source = if single_output {
            self.completed.scalar_node()
        } else {
            None
        };
        let domain = if let Some(source) = source {
            if let Some(types) = self.graph.domain(source).admissible_types() {
                let mut arrays = Vec::new();
                for ty in types {
                    arrays.push(ty.array_type());
                }
                InputTypeConstraint::types(arrays)
            } else {
                InputTypeConstraint::any()
            }
        } else {
            fallback.clone()
        };
        // Retain the ARRAY node across projection expansion so enclosing
        // operations keep their existing links to this result.
        let result = if let Some(node) = existing {
            self.graph.restrict(node, &domain);
            node
        } else {
            self.graph.node(&domain)
        };
        if let Some(source) = source {
            self.graph
                .project(source, result, TypeProjection::construct_array());
        }
        (result, domain)
    }
}

#[cfg(test)]
mod test_array_query_projection_preserves_both_source_shapes_and_correlations {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::PgBuiltInType,
        test_helpers::query::{expression as projection, resolve},
    };

    use std::collections::BTreeMap;

    #[test]
    fn array_query_projection_preserves_both_source_shapes_and_correlations() {
        let (query, context) = resolve(
            "SELECT CASE WHEN true THEN ARRAY(SELECT $1) ELSE $2::integer[] END",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraints = context.parameter_constraints();
        for input in [PgBuiltInType::int4(), PgBuiltInType::int4().array_type()] {
            assert!(constraints.allows(&BTreeMap::from([(1, input)])));
        }
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        assert_eq!(
            projection(&query).get_type(),
            Some(PgBuiltInType::int4().array_type())
        );
    }

    #[test]
    fn unconstrained_source_keeps_functional_array_relationship() {
        let (query, context) =
            resolve("SELECT ARRAY(SELECT $1)", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let constraints = context.parameter_constraints();
        assert!(constraints.parameter(1).unwrap().constraint().is_any());
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4().array_type())])));
        assert!(
            projection(&query)
                .as_array_subquery()
                .unwrap()
                .types()
                .is_any()
        );
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn scalar_result(&mut self, value: &SemScalarExpr) -> Option<SemScalarExpr> {
        let source = self.completed.scalar_node()?;
        let result = self.operation(
            "scalar subquery",
            std::slice::from_ref(value),
            &InputTypeConstraint::any(),
            value.is_not_null(),
        );
        self.graph
            .equal(source, result.constraint_node().expect("operation node"));
        Some(result)
    }

    pub(crate) fn enter_query(&mut self) {
        let scope = self
            .scopes
            .last()
            .map(QueryScope::nested)
            .unwrap_or_default();
        self.scopes.push(scope);
    }

    pub(crate) fn leave_query(&mut self) {
        self.completed = self.scopes.pop().expect("query scope").outputs;
    }

    pub(crate) fn output(&mut self, name: &str, value: &SemScalarExpr, text_projection: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.outputs.push(name, value, text_projection);
        }
    }

    pub(crate) fn cte(&mut self, name: &str, columns: &[String]) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.cte(name, &self.completed, columns);
        }
    }

    pub(crate) fn relation(&mut self, name: Option<&str>, alias: &str, columns: &[String]) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.relation(name, alias, columns);
        }
    }

    fn function_source(
        &mut self,
        expression: &SemScalarExpr,
        alias: &AliasSpec,
        columns: &ColumnCollection,
        context: &impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if expression.constraint_node().is_none() {
            return Ok(());
        }
        let mut outputs = Outputs::default();
        for name in columns.names() {
            let definition = columns.get_column_def(&name).expect("declared output");
            let value = self.table_function_column(context, expression, alias, definition)?;
            outputs.push(name.as_str(), &value, false);
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.derived(alias.name().as_str(), &outputs, &alias.inference_columns());
        }
        Ok(())
    }

    fn table_function_column(
        &mut self,
        context: &impl ParseContext,
        expression: &SemScalarExpr,
        alias: &AliasSpec,
        definition: &ColumnDefinition,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if definition.get_type().is_some() {
            return Ok(ColumnReferenceExpr::function_output(alias, definition));
        }
        let name = definition.name().as_ref().expect("named function column");
        let Some(call) = expression.function_call_snapshot() else {
            return Ok(self.member_access(expression, name.as_str()));
        };
        let scalar = if let Some(overload) = call.overload() {
            overload.has_scalar_table_output(context)
        } else {
            call.candidates().is_some_and(|candidates| {
                candidates
                    .iter()
                    .all(|overload| overload.has_scalar_table_output(context))
            })
        };
        if scalar {
            return Ok(expression.clone());
        }
        let Some(candidates) = call.candidates() else {
            return Ok(self.member_access(expression, name.as_str()));
        };
        let (_, arguments) = call.inference_call();
        let types = ArgumentBindingCollection::from_expr_list(&arguments)
            .with_explicit_variadic(call.modifiers().variadic());
        let signatures = candidates.column_signatures(
            context,
            &types,
            &ColumnName::from(alias.name().as_str()),
            name,
        );
        self.call(context, &call.name().to_string(), &arguments, &signatures)
    }

    pub(crate) fn derived(&mut self, alias: &str, columns: &[String]) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.derived(alias, &self.completed, columns);
        }
    }

    pub(crate) fn column(&self, original: &SemScalarExpr) -> Option<SemScalarExpr> {
        if let Some(index) = original.output_index() {
            return self
                .scopes
                .last()?
                .outputs
                .0
                .get(index)?
                .column(original, &self.graph);
        }
        let (alias, _) = original.constraint_binding()?;
        for scope in self.scopes.iter().rev() {
            if scope.has_source(alias) {
                return scope.column(original, &self.graph);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Signature {
    inputs: Vec<TypeReference>,
    result: TypeReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    declared_inputs: Option<Vec<TypeReference>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unrestricted_inputs: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exact_inputs: Vec<usize>,
}
impl Signature {
    pub(crate) fn new(inputs: &[TypeReference], result: &TypeReference) -> Self {
        Self {
            inputs: inputs.to_vec(),
            result: result.clone(),
            declared_inputs: None,
            unrestricted_inputs: Vec::new(),
            exact_inputs: Vec::new(),
        }
    }
    pub(crate) fn new_function(
        inputs: &[TypeReference],
        declared_inputs: &[TypeReference],
        unrestricted_inputs: &[usize],
        result: &TypeReference,
    ) -> Option<Self> {
        if inputs.len() != declared_inputs.len() {
            return None;
        }
        let mut signature = Self::new(inputs, result);
        signature.declared_inputs = Some(declared_inputs.to_vec());
        signature.unrestricted_inputs = unrestricted_inputs.to_vec();
        if signature.is_concrete_for_arity(inputs.len()) {
            Some(signature)
        } else {
            None
        }
    }

    pub(super) fn is_concrete_for_arity(&self, arity: usize) -> bool {
        self.inputs.len() == arity
            && self
                .inputs
                .iter()
                .chain([&self.result])
                .all(Self::concrete_type)
    }

    fn concrete_type(ty: &TypeReference) -> bool {
        !ty.is_dynamic() && !ty.is_simple_infer_arg() && !ty.is_common_infer_arg()
    }

    pub(super) fn accepts_any_type(&self, index: usize) -> bool {
        self.unrestricted_inputs.contains(&index)
    }

    pub(crate) fn simple_function_template(
        inputs: &[TypeReference],
        declarations: &[TypeReference],
        result: &TypeReference,
    ) -> Option<Self> {
        let mut signature = Self::new(inputs, result);
        signature.declared_inputs = Some(declarations.to_vec());
        if signature.is_simple_function_template() {
            Some(signature.with_simple_identity())
        } else {
            None
        }
    }
    pub(crate) fn is_simple_function_template(&self) -> bool {
        self.declared_inputs.is_some()
            && self.inputs.iter().any(TypeReference::is_simple_infer_arg)
            && self.inputs.iter().chain([&self.result]).all(|ty| {
                !ty.is_dynamic()
                    && !ty.is_common_infer_arg()
                    && !ty.is_any_range()
                    && !ty.is_any_multi_range()
            })
    }
    pub(crate) fn instantiate_simple(
        &self,
        context: &impl ParseContext,
        source: &TypeReference,
    ) -> Vec<Self> {
        let mut seeds = vec![(source.clone(), None)];
        if let Some(element) = context.get_array_item_type(source) {
            seeds.push((element, Some(source)));
        }
        let mut signatures = Vec::new();
        for (element, array) in seeds {
            if context.get_array_item_type(&element).is_some()
                && self.inputs.iter().any(TypeReference::is_any_non_array)
            {
                continue;
            }
            if self.inputs.iter().any(TypeReference::is_any_array) {
                let candidate = array.cloned().unwrap_or_else(|| element.array_type());
                // array_type() preserves dimensional identity. An array operand
                // cannot thereby become its own element in an anyarray input.
                if context.get_array_item_type(&candidate).as_ref() != Some(&element) {
                    continue;
                }
            }
            let substitution = SimpleInferResult::new(&element, array);
            let mut signature = self.clone();
            for input in &mut signature.inputs {
                substitution.apply(input);
            }
            substitution.apply(&mut signature.result);
            if signature.is_concrete_for_arity(self.inputs.len()) {
                signatures.push(signature);
            }
        }
        signatures
    }

    pub(crate) fn with_simple_identity(mut self) -> Self {
        if let Some(declarations) = &self.declared_inputs {
            self.exact_inputs = declarations
                .iter()
                .enumerate()
                .filter_map(|(index, ty)| {
                    if ty.is_simple_infer_arg() {
                        Some(index)
                    } else {
                        None
                    }
                })
                .collect();
        }
        self
    }

    pub(crate) fn cast_score(
        &self,
        catalog: &impl BaseParseContext,
        source: &TypeReference,
        index: usize,
    ) -> Option<OverloadRank> {
        if self.exact_inputs.contains(&index) && source != &self.inputs[index] {
            return None;
        }
        if self.accepts_any_type(index) {
            return Some(OverloadRank::default());
        }
        if let Some(declared_inputs) = &self.declared_inputs {
            OverloadRank::function_argument(
                catalog,
                Some(source),
                Some(&self.inputs[index]),
                Some(&declared_inputs[index]),
            )
        } else {
            OverloadRank::operator_argument(catalog, source, &self.inputs[index])
        }
    }

    pub(super) fn is_array_comparison(&self) -> bool {
        self.inputs.len() == 2
            && self
                .inputs
                .iter()
                .all(|ty| !ty.is_dynamic() && ty.is_any_array())
            && Self::concrete_type(&self.result)
    }

    pub(super) fn inputs(&self) -> &[TypeReference] {
        &self.inputs
    }

    pub(super) const fn result(&self) -> &TypeReference {
        &self.result
    }
}

// Only the direct projection of a set operand defers standalone text
// defaulting. Nested derived queries retain their own projection boundary.
#[derive(Default)]
struct SetOperandScopes {
    depth: usize,
    operands: Vec<usize>,
}
impl SetOperandScopes {
    const fn enter_query(&mut self) {
        self.depth += 1;
    }
    const fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn enter_operand(&mut self) {
        self.operands.push(self.depth + 1);
    }
    fn leave_operand(&mut self) {
        self.operands.pop();
    }
    fn is_operand(&self) -> bool {
        self.operands.last() == Some(&self.depth)
    }
}

struct Collector<'context, 'cache, C: ParseContext> {
    set_scopes: SetOperandScopes,
    dml_depth: usize,
    parameters: ParameterResolution<'cache>,
    context: &'context mut C,
}

impl<C: ParseContext> crate::sem::traversal::SemanticVisitorMut for Collector<'_, '_, C> {
    fn table_function(
        &mut self,
        expression: &mut SemScalarExpr,
        alias: &AliasSpec,
        columns: &mut ColumnCollection,
    ) -> Result<(), AnalysisError> {
        self.parameters
            .function_source(expression, alias, columns, self.context)?;
        Ok(())
    }

    fn enter_expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.resolve_between(self.context)
    }

    fn membership(&mut self, value: &mut MembershipExpr) -> Result<(), AnalysisError> {
        if value.expand_pending_stars(self.context)? {
            value.walk(self)?;
        }
        Ok(())
    }
    fn array_subquery(&mut self, value: &mut ArraySubqueryExpr) -> Result<(), AnalysisError> {
        if value.expand_pending_stars(self.context)? {
            value.walk(self)?;
        }
        Ok(())
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.collect_constraints(&mut self.parameters, self.context)
    }
    fn sampling_input(&mut self, value: &SemScalarExpr, target: &TypeReference) {
        self.parameters
            .require_assignment(self.context, value, target);
    }

    fn assignment(
        &mut self,
        value: &mut SemScalarExpr,
        target: &InsertTarget,
    ) -> Result<(), AnalysisError> {
        if let Some(ty) = target.assignment_type() {
            self.assignment_value(value, &ty)?;
        }
        Ok(())
    }
    fn assignment_value(
        &mut self,
        value: &mut SemScalarExpr,
        ty: &TypeReference,
    ) -> Result<(), AnalysisError> {
        if self.context.dialect() == Dialect::postgres() {
            if value.constraint_node().is_some() {
                self.parameters.require_assignment(self.context, value, ty);
            } else if let Some(source) = value.get_type()
                && &source != ty
                && self.context.get_assignment_cast(&source, ty).is_none()
            {
                self.context
                    .report_analysis_problem(AnalysisProblem::parameter_constraint(&format!(
                        "cannot assign {source} to {ty}"
                    )))?;
            }
            if value.get_type().as_ref() != Some(ty) {
                *value = SemScalarExpr::wrap_implicit_cast(ty, value);
            }
        }
        Ok(())
    }
    fn dml_returning(&mut self, columns: &mut ResultColumnCollection) -> Result<(), AnalysisError> {
        if self.context.dialect() == Dialect::postgres() {
            self.parameters.solve(self.context)?;
        }
        columns.walk(self)?;
        if self.context.dialect() == Dialect::postgres() {
            self.parameters.solve(self.context)?;
            columns.default_returning_parameters(&mut self.parameters);
        }
        Ok(())
    }
    fn subscript(&mut self, value: &SemScalarExpr) {
        if let Some(node) = value.constraint_node() {
            self.parameters.graph.restrict(
                node,
                &InputTypeConstraint::types(
                    self.context.assignment_sources(&PgBuiltInType::int4()),
                ),
            );
        }
    }
    fn enter_dml(&mut self) {
        self.dml_depth += 1;
    }
    fn leave_dml(&mut self) {
        self.dml_depth -= 1;
    }
    fn enter_set_operand(&mut self) {
        self.set_scopes.enter_operand();
    }
    fn leave_set_operand(&mut self) {
        self.set_scopes.leave_operand();
    }
    fn enter_query(&mut self) {
        self.set_scopes.enter_query();
        self.parameters.enter_query();
    }
    fn leave_query(&mut self) {
        if self.dml_depth > 0
            && !self.set_scopes.is_operand()
            && self.context.dialect() == Dialect::postgres()
        {
            self.parameters.default_query_outputs(self.context);
        }
        self.parameters.leave_query();
        self.set_scopes.leave_query();
    }
    fn cte(&mut self, name: &str) {
        self.parameters.cte(name, &[]);
    }
    fn relation(&mut self, name: Option<&str>, alias: &str, columns: &[String]) {
        self.parameters.relation(name, alias, columns);
    }
    fn derived(&mut self, alias: &str, columns: &[String]) {
        self.parameters.derived(alias, columns);
    }
    fn output(&mut self, name: &str, expression: &mut SemScalarExpr) {
        self.parameters.output(
            name,
            expression,
            self.context.dialect() == Dialect::postgres()
                && !self.set_scopes.is_operand()
                && expression.is_unconstrained_parameter(),
        );
    }
    fn predicate(&mut self, value: &SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres() {
            self.parameters
                .require(self.context, value, &PgBuiltInType::bool(), false);
        }
    }
    fn order_operator(
        &mut self,
        value: &SemScalarExpr,
        operator: &ResolvedOperator,
    ) -> Result<(), AnalysisError> {
        operator.infer(&mut self.parameters, self.context, Some(value), Some(value))?;
        Ok(())
    }
    fn row_count(&mut self, value: &SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres() {
            self.parameters
                .require(self.context, value, &PgBuiltInType::int8(), false);
        }
    }
    fn window_definition(&mut self, window: &QueryWindow) {
        if let Some(scope) = self.parameters.scopes.last_mut() {
            scope
                .window_orders
                .push(window.ordering_type(&scope.window_orders));
        }
    }
    fn frame_offset(
        &mut self,
        value: &SemScalarExpr,
        mode: &str,
        order: &[QueryOrder],
        reference: Option<&QueryName>,
    ) {
        if self.context.dialect() != Dialect::postgres() {
            return;
        }
        if mode == "ROWS" || mode == "GROUPS" {
            self.row_count(value);
            return;
        }
        let ty = match order {
            [order] => order.expression().get_type(),
            [] => {
                if let Some(reference) = reference
                    && let Some(index) = reference.resolution().window_index()
                    && let Some(scope) = self.parameters.scopes.last()
                {
                    scope.window_orders.get(index).cloned().flatten()
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(ty) = ty {
            let target = if [
                TypeReference::from_master_type_name("date"),
                TypeReference::from_master_type_name("timestamp"),
                TypeReference::from_master_type_name("timestamptz"),
                TypeReference::from_master_type_name("time"),
                TypeReference::from_master_type_name("timetz"),
            ]
            .contains(&ty)
            {
                TypeReference::from_master_type_name("interval")
            } else if [
                "int2", "int4", "int8", "numeric", "float4", "float8", "interval",
            ]
            .map(TypeReference::from_master_type_name)
            .contains(&ty)
            {
                ty
            } else {
                // Custom RANGE operator classes need their in_range metadata;
                // equal ordering and offset types cannot be assumed.
                return;
            };
            self.parameters.require(self.context, value, &target, false);
        }
    }
    fn set_operation(&mut self, query: &mut SetOperationQuery) -> Result<(), AnalysisError> {
        query.collect_constraints(&mut self.parameters, self.context)
    }
}

struct Finalizer<'a, C: ParseContext> {
    set_scopes: SetOperandScopes,
    constraints: &'a ParameterConstraints,
    context: &'a mut C,
    expanded_arrays: bool,
}
impl<'a, C: ParseContext> Finalizer<'a, C> {
    fn new(constraints: &'a ParameterConstraints, context: &'a mut C) -> Self {
        Self {
            constraints,
            context,
            set_scopes: SetOperandScopes::default(),
            expanded_arrays: false,
        }
    }
}
impl<C: ParseContext> crate::sem::traversal::SemanticVisitorMut for Finalizer<'_, C> {
    fn table_function(
        &mut self,
        expression: &mut SemScalarExpr,
        alias: &AliasSpec,
        columns: &mut ColumnCollection,
    ) -> Result<(), AnalysisError> {
        if let Some(call) = expression.function_call_snapshot()
            && let Some(overload) = call.overload()
        {
            columns.refresh_function_output(&overload.table_function_output_columns(
                self.context,
                &ColumnName::from(alias.name().as_str()),
            ));
        }
        Ok(())
    }

    fn enter_set_operand(&mut self) {
        self.set_scopes.enter_operand();
    }
    fn leave_set_operand(&mut self) {
        self.set_scopes.leave_operand();
    }
    fn enter_query(&mut self) {
        self.set_scopes.enter_query();
    }
    fn leave_query(&mut self) {
        self.set_scopes.leave_query();
    }

    fn set_operation(&mut self, query: &mut SetOperationQuery) -> Result<(), AnalysisError> {
        query.apply_constraints(self.constraints);
        Ok(())
    }

    fn table_sample(&mut self, sample: &mut TableSampleSource) -> Result<(), AnalysisError> {
        sample.validate_inputs(self.context)
    }
    fn membership(&mut self, value: &mut MembershipExpr) -> Result<(), AnalysisError> {
        if value.expand_pending_stars(self.context)? {
            self.expanded_arrays = true;
            value.walk(self)?;
        }
        Ok(())
    }
    fn array_subquery(&mut self, value: &mut ArraySubqueryExpr) -> Result<(), AnalysisError> {
        if value.expand_pending_stars(self.context)? {
            self.expanded_arrays = true;
            value.walk(self)?;
        }
        Ok(())
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.apply_constraints(self.constraints);
        expression.finalize_call(self.context)?;
        expression.validate_collation(self.context)
    }
    fn output(&mut self, _name: &str, expression: &mut SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres()
            && !self.set_scopes.is_operand()
            && expression.is_unconstrained_parameter()
        {
            *expression = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::text(), expression);
        }
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn infer_query(
        query: &mut SelectStatement,
        context: &mut impl ParseContext,
        cache: &'cache mut CacheContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            set_scopes: SetOperandScopes::default(),
            dml_depth: 0,
            parameters: Self::new(cache),
            context,
        };
        let mut expand_root = true;
        loop {
            query.walk(&mut collector)?;
            collector.parameters.solve(collector.context)?;
            let constraints = collector.parameters.constraints();
            let mut finalizer = Finalizer::new(&constraints, collector.context);
            query.walk(&mut finalizer)?;
            let expanded_arrays = finalizer.expanded_arrays;
            let expanded_root = if expand_root || expanded_arrays {
                query.expand_pending_stars(collector.context)?
            } else {
                false
            };
            expand_root = expanded_arrays;
            if !expanded_arrays && !expanded_root {
                return collector
                    .parameters
                    .complete(collector.context, constraints);
            }
        }
    }
}

#[cfg(test)]
mod test_uses_clause_specific_constraints {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::OperatorName,
        sem::{
            PgBuiltInType, TypeReference as SemTypeReference,
            parameter_resolution::ParameterResolution,
        },
        test_helpers::{
            create_cache_context,
            query::{output_type, resolve},
        },
    };

    #[rstest::rstest]
    #[case::inherited_numeric(
        "SELECT sum(1) OVER (w RANGE $7 PRECEDING) WINDOW w AS (ORDER BY 1)",
        PgBuiltInType::int4()
    )]
    #[case::range_date(
        "SELECT sum(1) OVER (ORDER BY $8::date RANGE $7 PRECEDING)",
        crate::sem::TypeReference::from_master_type_name("interval")
    )]
    #[case::using("SELECT 1 ORDER BY $7 USING <", PgBuiltInType::int4())]
    fn uses_clause_specific_constraints(#[case] sql: &str, #[case] ty: SemTypeReference) {
        let (_, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let domain = context
            .parameter_constraints()
            .parameter(7)
            .unwrap()
            .constraint();
        assert!(!domain.is_any());
        assert!(domain.allows(&ty));
        assert!(!domain.allows(&PgBuiltInType::text()));
    }

    #[test]
    fn values_refresh_result_metadata_after_inference() {
        let (query, context) = resolve("VALUES ($1 + 1)", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(output_type(&query, "column1"), Some(PgBuiltInType::int4()));
    }

    #[test]
    fn inference_uses_captured_function_and_operator_bindings() {
        let (mut query, mut catalog) = resolve(
            "SELECT sum($1 + 1) FILTER (WHERE $2) ORDER BY $3 + 1",
            Dialect::postgres(),
            false,
        )
        .unwrap();
        catalog.setup_function("sum", &[]);
        catalog.set_get_operator_op_result(&OperatorName::from_parts(None, "+"), true, true, None);
        catalog.begin_parameter_type_analysis();
        let constraints =
            ParameterResolution::infer_query(&mut query, &mut catalog, &mut create_cache_context())
                .unwrap();
        catalog.finish_analysis(Ok(())).unwrap();
        assert_eq!(catalog.reported_problem_count(), 0);
        assert!(
            constraints
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
        assert!(!constraints.parameter(1).unwrap().constraint().is_any());
        assert_eq!(
            constraints
                .parameter(2)
                .unwrap()
                .constraint()
                .admissible_types(),
            Some([PgBuiltInType::bool()].as_slice())
        );
        assert!(!constraints.parameter(3).unwrap().constraint().is_any());
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn infer_insert(
        insert: &mut InsertStatement,
        context: &mut impl ParseContext,
        cache: &'cache mut CacheContext,
        implicit_targets: bool,
    ) -> Result<(ParameterConstraints, bool), AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            set_scopes: SetOperandScopes::default(),
            dml_depth: 0,
            parameters: Self::new(cache),
            context,
        };
        let mut expand_source = true;
        let mut expanded_source = false;
        loop {
            insert.walk(&mut collector)?;
            collector.parameters.solve(collector.context)?;
            let constraints = collector.parameters.constraints();
            let mut finalizer = Finalizer::new(&constraints, collector.context);
            insert.walk(&mut finalizer)?;
            let expanded_arrays = finalizer.expanded_arrays;
            let expanded = if expand_source || expanded_arrays {
                insert.expand_source_stars(collector.context)?
            } else {
                false
            };
            expand_source = expanded_arrays;
            expanded_source |= expanded;
            if implicit_targets && expanded {
                insert.truncate_implicit_targets();
            }
            if !expanded_arrays && !expanded {
                return Ok((
                    collector
                        .parameters
                        .complete(collector.context, constraints)?,
                    expanded_source,
                ));
            }
        }
    }
}

#[cfg(test)]
mod test_infers_assignment_parameters {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::{PgBuiltInType, SemAst},
        test_helpers::{
            create_cache_context as test_helpers_create_cache_context, insert as test_helpers,
            insert::analyze,
        },
    };

    #[rstest::rstest]
    #[case::cte("WITH c AS (SELECT $1 n) INSERT INTO items(id) SELECT n FROM c")]
    #[case::value("INSERT INTO items(id) VALUES ($1) RETURNING id")]
    #[case::select("INSERT INTO items(id) SELECT $1 RETURNING id")]
    #[case::later_row("INSERT INTO items(id) VALUES (1), ($1) RETURNING id")]
    #[case::repeated("INSERT INTO items(id) VALUES ($1), ($1) RETURNING id")]
    #[case::compound("INSERT INTO items(id) SELECT $1 UNION ALL SELECT $1 RETURNING id")]
    fn infers_assignment_parameters(#[case] sql: &str) {
        let (_, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        let constraint = context
            .parameter_constraints()
            .parameter(1)
            .expect("parameter");
        assert!(constraint.constraint().allows(&PgBuiltInType::int4()));
        assert!(!constraint.constraint().allows(&PgBuiltInType::text()));
    }

    #[rstest::rstest]
    #[case::value("INSERT INTO items(id) VALUES($1)")]
    #[case::subscript("INSERT INTO arrays(a[$1]) VALUES(1)")]
    fn assignment_casts_do_not_become_overload_implicit_casts(#[case] sql: &str) {
        use crate::sem::{CastContext, CastDefinition};
        let mut context = test_helpers::catalog(Dialect::postgres());
        context.setup_type(PgBuiltInType::int8());
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int8(),
            &PgBuiltInType::int4(),
            Some(CastDefinition::new(CastContext::assignment())),
        );
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        SemAst::from_ast_node(
            statement.node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        )
        .unwrap();
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int8())
        );
        assert!(
            !crate::context::CastInfoRead::get_implicit_cast(
                &context,
                &PgBuiltInType::int8(),
                &PgBuiltInType::int4()
            )
            .unwrap()
            .use_in_implicit()
        );
        assert_eq!(context.reported_problem_count(), 0);
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn infer_update(
        update: &mut UpdateStatement,
        context: &mut impl ParseContext,
        cache: &'cache mut CacheContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            set_scopes: SetOperandScopes::default(),
            dml_depth: 0,
            parameters: Self::new(cache),
            context,
        };
        loop {
            update.walk(&mut collector)?;
            collector.parameters.solve(collector.context)?;
            let constraints = collector.parameters.constraints();
            let mut finalizer = Finalizer::new(&constraints, collector.context);
            update.walk(&mut finalizer)?;
            let expanded_arrays = finalizer.expanded_arrays;
            if !expanded_arrays {
                return collector
                    .parameters
                    .complete(collector.context, constraints);
            }
        }
    }
}

#[cfg(test)]
mod test_infers_parameters {
    use crate::{
        ast::Dialect, context::ParseContext, sem::PgBuiltInType, test_helpers::update::analyze,
    };

    #[rstest::rstest]
    #[case::set("UPDATE items SET id=$1")]
    #[case::cte("WITH s AS (SELECT $1::integer n) UPDATE items SET id=s.n FROM s")]
    #[case::subquery("UPDATE items SET id=(SELECT $1::integer)")]
    #[case::row_subquery("UPDATE items SET (id,label)=(SELECT $1::integer,'two')")]
    #[case::cte_returning("WITH u AS (UPDATE items SET id=$1 RETURNING id) SELECT id FROM u")]
    fn infers_parameters(#[case] sql: &str) {
        let (_, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraint = context.parameter_constraints().parameter(1).unwrap();
        assert!(constraint.constraint().allows(&PgBuiltInType::int4()));
        assert!(!constraint.constraint().allows(&PgBuiltInType::bool()));
    }
}

#[cfg(test)]
mod test_synthetic_array_parameters_retain_assignment_compatibility {
    use crate::{
        ast::Dialect,
        parse_statements,
        sem::SemAst,
        test_helpers::{create_cache_context, update_assignments as test_helpers},
    };

    #[test]
    fn synthetic_array_parameters_retain_assignment_compatibility() {
        let mut context = test_helpers::catalog();
        let result = SemAst::from_ast_node(
            parse_statements("UPDATE arrays SET a[$1]=$2", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap()
                .node(),
            &mut context,
            &mut create_cache_context(),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(context.reported_problem_count(), 0);
    }
}

#[cfg(test)]
mod test_inferred_types_reach_stored_assignments_predicates_and_outputs {
    use crate::{ast::Dialect, sem::PgBuiltInType, test_helpers::update::analyze};

    #[test]
    fn inferred_types_reach_stored_assignments_predicates_and_outputs() {
        let (statement, context) = analyze(
            "UPDATE items SET id=$1 WHERE $2 RETURNING id AS n",
            Dialect::postgres(),
        );
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let update = statement.as_update().unwrap();
        assert_eq!(
            update.assignments()[0]
                .value()
                .as_expression()
                .unwrap()
                .get_type(),
            Some(PgBuiltInType::int4())
        );
        assert_eq!(
            update.predicate().unwrap().get_type(),
            Some(PgBuiltInType::bool())
        );
        assert_eq!(
            update.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int4())
        );
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn infer_delete(
        delete: &mut DeleteStatement,
        context: &mut impl ParseContext,
        cache: &'cache mut CacheContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            set_scopes: SetOperandScopes::default(),
            dml_depth: 0,
            parameters: Self::new(cache),
            context,
        };
        loop {
            delete.walk(&mut collector)?;
            collector.parameters.solve(collector.context)?;
            let constraints = collector.parameters.constraints();
            let mut finalizer = Finalizer::new(&constraints, collector.context);
            delete.walk(&mut finalizer)?;
            let expanded_arrays = finalizer.expanded_arrays;
            if !expanded_arrays {
                return collector
                    .parameters
                    .complete(collector.context, constraints);
            }
        }
    }
}

#[cfg(test)]
mod test_infers_parameters_delete_statement {

    use crate::{
        ast::Dialect, context::ParseContext, sem::PgBuiltInType, test_helpers::delete::analyze,
    };

    #[rstest::rstest]
    #[case::where_parameter("DELETE FROM items WHERE $1", true)]
    #[case::returning("DELETE FROM items RETURNING $1::integer", false)]
    #[case::with(
        "WITH s AS (SELECT $1::integer n) DELETE FROM items USING s RETURNING s.n",
        false
    )]
    #[case::nested("DELETE FROM items RETURNING (SELECT $1::integer)", false)]
    #[case::cte_returning(
        "WITH d(n) AS (DELETE FROM items RETURNING $1::integer) SELECT n FROM d",
        false
    )]
    fn infers_parameters(#[case] sql: &str, #[case] boolean: bool) {
        let (_, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraint = context.parameter_constraints().parameter(1).unwrap();
        assert_eq!(
            constraint.constraint().allows(&PgBuiltInType::bool()),
            boolean
        );
        assert_eq!(
            constraint.constraint().allows(&PgBuiltInType::int4()),
            !boolean
        );
    }

    #[test]
    fn synthetic_parameter_conflict_retains_its_diagnostic() {
        let (_, context) = analyze(
            "DELETE FROM items WHERE $1 RETURNING $1::integer",
            Dialect::postgres(),
        );
        assert!(context.reported_problem_count() > 0);
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn infer_merge(
        merge: &mut MergeStatement,
        context: &mut impl ParseContext,
        cache: &'cache mut CacheContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            set_scopes: SetOperandScopes::default(),
            dml_depth: 0,
            parameters: Self::new(cache),
            context,
        };
        loop {
            merge.walk(&mut collector)?;
            collector.parameters.solve(collector.context)?;
            let constraints = collector.parameters.constraints();
            let mut finalizer = Finalizer::new(&constraints, collector.context);
            merge.walk(&mut finalizer)?;
            let expanded_arrays = finalizer.expanded_arrays;
            if !expanded_arrays {
                return collector
                    .parameters
                    .complete(collector.context, constraints);
            }
        }
    }
}

#[cfg(test)]
mod test_infers_parameters_across_ordered_actions_and_returning {
    use crate::{
        context::ParseContext,
        sem::{
            PgBuiltInType,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::{document_merge::catalog, statements::resolve_postgres as resolve},
    };

    #[test]
    fn infers_parameters_across_ordered_actions_and_returning() {
        let mut context = catalog(17);
        let before = parameter_resolution_inference_runs();
        let statement = resolve("MERGE INTO items t USING incoming s ON $2 WHEN MATCHED AND $3 THEN UPDATE SET id=$1 WHEN NOT MATCHED THEN INSERT(id) VALUES($1) RETURNING $1::integer AS value", &mut context).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
        assert!(
            context
                .parameter_constraints()
                .parameter(2)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::bool())
        );
        assert_eq!(
            statement
                .as_merge()
                .unwrap()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::int4())
        );
    }
}

#[cfg(test)]
mod test_reports_parameter_conflicts_across_when_actions {
    use crate::test_helpers::merge_assignments::assert_rejected;

    #[test]
    fn reports_parameter_conflicts_across_when_actions() {
        assert_rejected(
            "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=$1 WHEN NOT MATCHED THEN INSERT(label) VALUES($1)",
            17,
        );
    }
}

impl<'cache> ParameterResolution<'cache> {
    pub(crate) fn infer_expression(
        expression: &mut SemScalarExpr,
        context: &mut impl ParseContext,
        cache: &'cache mut CacheContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            set_scopes: SetOperandScopes::default(),
            dml_depth: 0,
            parameters: Self::new(cache),
            context,
        };
        loop {
            expression.walk(&mut collector)?;
            collector.parameters.solve(collector.context)?;
            let constraints = collector.parameters.constraints();
            let mut finalizer = Finalizer::new(&constraints, collector.context);
            expression.walk(&mut finalizer)?;
            let expanded_arrays = finalizer.expanded_arrays;
            if !expanded_arrays {
                return collector
                    .parameters
                    .complete(collector.context, constraints);
            }
        }
    }

    pub(crate) fn operator(
        &mut self,
        context: &mut impl ParseContext,
        name: &OperatorName,
        left: Option<&SemScalarExpr>,
        right: Option<&SemScalarExpr>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if left.is_some_and(|arg| arg.constraint_node().is_none())
            && right.is_some_and(|arg| arg.constraint_node().is_none())
        {
            return Ok(None);
        }
        if context.dialect() == Dialect::sqlite() {
            return Ok(Some(self.sqlite_operator(name, left, right)));
        }
        let arguments: Vec<_> = left.into_iter().chain(right).cloned().collect();
        if let Some(operator) = context.get_operator(name, left.is_some(), right.is_some()) {
            let signatures = operator.parameter_signatures();
            return Ok(Some(self.call_with_diagnostic_name(
                context,
                &name.to_string(),
                &name.to_name_string(),
                &arguments,
                &signatures,
            )?));
        }
        Ok(Some(self.operation(
            &name.to_string(),
            &arguments,
            &InputTypeConstraint::any(),
            None,
        )))
    }
}

#[cfg(test)]
std::thread_local! {
    static INFERENCE_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(crate) fn inference_runs() -> usize {
    INFERENCE_RUNS.with(std::cell::Cell::get)
}
