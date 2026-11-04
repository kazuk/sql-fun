//! Parameter inference over the name-resolved semantic tree.

#[cfg(test)]
mod case_tests;
#[cfg(test)]
mod indirection_tests;

use crate::{
    CacheContext,
    names::OperatorName,
    sem::{
        AnalysisProblem, ArraySubqueryExpr, DeleteStatement, InsertStatement, InsertTarget,
        MembershipExpr, MergeStatement, MinMaxExpr, ParseContext, PgBuiltInType, QueryName,
        QueryOrder, QueryWindow, ResolvedOperator, ResultColumnCollection, SelectStatement,
        SetOperationQuery, TableSampleSource, TruthTestExpr, UpdateStatement,
        parameter_constraints::TypeProjection, problem::MinMaxProblem,
    },
};

use crate::{
    ast::Dialect,
    context::BaseParseContext,
    sem::{
        AnalysisError, ConstrainedExpr, ConstraintGraph, InputTypeConstraint, ParameterConstraints,
        SemScalarExpr, SignatureAlternative, TypeReference, overload_rank::OverloadRank,
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
}
impl Signature {
    pub(crate) fn new(inputs: &[TypeReference], result: &TypeReference) -> Self {
        Self {
            inputs: inputs.to_vec(),
            result: result.clone(),
            declared_inputs: None,
            unrestricted_inputs: Vec::new(),
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

    pub(crate) fn cast_score(
        &self,
        catalog: &impl BaseParseContext,
        source: &TypeReference,
        index: usize,
    ) -> Option<OverloadRank> {
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
