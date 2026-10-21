//! Parameter inference over the name-resolved semantic tree.

#[cfg(test)]
mod case_tests;
#[cfg(test)]
mod indirection_tests;
use crate::ast::Dialect;
use crate::sem::overload_rank::OverloadRank;
use crate::sem::{
    AnalysisError, BaseParseContext, ConstrainedExpr, ConstraintGraph, InputTypeConstraint,
    ParameterConstraints, SemScalarExpr, SignatureAlternative, TypeReference,
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
            InputTypeConstraint::exact(&crate::sem::PgBuiltInType::text())
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

#[derive(Default)]
pub(crate) struct ParameterResolution {
    graph: ConstraintGraph,
    unsupported: Vec<crate::sem::AnalysisProblem>,
    scopes: Vec<QueryScope>,
    completed: Outputs,
}

impl ParameterResolution {
    pub(crate) fn constraints(&self) -> ParameterConstraints {
        let mut constraints = ParameterConstraints::from_graph(&self.graph);
        constraints.set_outputs(&self.completed.constraints(&self.graph));
        constraints
    }

    fn solve(&mut self, context: &mut impl crate::sem::ParseContext) -> Result<(), AnalysisError> {
        for problem in std::mem::take(&mut self.unsupported) {
            context.report_analysis_problem(problem)?;
        }
        self.graph.prepare_projections(context);
        self.graph.report_projection_problems(context)?;
        if let Err(problem) = self.graph.solve() {
            crate::sem::ParseContext::report_analysis_problem(
                context,
                crate::sem::AnalysisProblem::parameter_constraint(problem.message()),
            )?;
        }
        Ok(())
    }

    fn default_query_outputs(&mut self, context: &impl crate::sem::ParseContext) {
        self.graph.prepare_projections(context);
        // UPDATE/DELETE subqueries resolve unknown projected parameters before
        // surrounding expressions or assignments constrain them. Keep solver failure
        // in the graph for the final diagnostic-producing solve.
        let _ = self.graph.solve();
        if let Some(scope) = self.scopes.last() {
            for output in &scope.outputs.0 {
                if let Some(node) = output.node
                    && self.graph.domain(node).is_any()
                    && output.domain
                        == InputTypeConstraint::exact(&crate::sem::PgBuiltInType::text())
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
            self.graph.restrict(
                node,
                &InputTypeConstraint::exact(&crate::sem::PgBuiltInType::text()),
            );
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
        context: &mut impl crate::sem::ParseContext,
        results: &[SemScalarExpr],
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        self.common_type(context, "CASE", results, false)
    }

    pub(crate) fn case_condition(
        &mut self,
        context: &impl crate::sem::ParseContext,
        condition: &SemScalarExpr,
    ) -> usize {
        let node = self.input(condition);
        // Bare NULL/string literals have PostgreSQL unknown type and can be
        // coerced to boolean. Parameters instead retain their source domain.
        if crate::sem::MinMaxExpr::input_type(condition) != Some(crate::sem::MinMaxExpr::unknown())
        {
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(
                    context.cast_sources(&crate::sem::PgBuiltInType::bool(), false),
                ),
            );
        }
        node
    }
    pub(crate) fn unify_coalesce_types(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        self.common_type(context, "COALESCE", arguments, false)
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
            crate::sem::parameter_constraints::TypeProjection::member(member),
        );
        result
    }
    pub(crate) fn array_access(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
        base: &SemScalarExpr,
        bounds: &[&SemScalarExpr],
        slice: bool,
    ) -> SemScalarExpr {
        for bound in bounds {
            let node = self.input(bound);
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(
                    context.assignment_sources(&crate::sem::PgBuiltInType::int4()),
                ),
            );
        }
        let source = self.input(base);
        let result = self.operation(
            "array access",
            &[],
            &InputTypeConstraint::any(),
            base.is_not_null(),
        );
        let projection = if slice {
            crate::sem::parameter_constraints::TypeProjection::array()
        } else {
            crate::sem::parameter_constraints::TypeProjection::array_element()
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
        _name: &crate::sem::OperatorName,
        _left: Option<&SemScalarExpr>,
        _right: Option<&SemScalarExpr>,
    ) -> SemScalarExpr {
        todo!("SQLite operator type constraints");
    }
    pub(crate) fn special_operator(&mut self, operation: &str, _arguments: &[&SemScalarExpr]) {
        self.unsupported
            .push(crate::sem::AnalysisProblem::query_analysis(
                ::sql_fun_core::analysis_problem::QueryAnalysisStage::parameter_semantics(),
                &format!("parameter constraints for {operation} are not implemented (#513)"),
            ));
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
                &InputTypeConstraint::types(catalog.cast_sources(target, explicit)),
            );
        }
    }

    pub(crate) fn call(
        &mut self,
        catalog: &impl BaseParseContext,
        name: &str,
        arguments: &[SemScalarExpr],
        signatures: &[Signature],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let result = self.operation(name, arguments, &InputTypeConstraint::any(), None);
        let mut nodes = Vec::new();
        let mut domains = Vec::new();
        for argument in arguments {
            let node = self.input(argument);
            domains.push(self.graph.domain(node).clone());
            nodes.push(node);
        }
        nodes.push(result.constraint_node().expect("operation has node"));
        let alternatives = Signature::alternatives(signatures, catalog, &domains);
        self.graph.overload(&nodes, &alternatives, name);
        Ok(result)
    }

    pub(crate) fn min_max(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
        operation: &str,
        arguments: &[SemScalarExpr],
    ) -> Result<(usize, InputTypeConstraint), AnalysisError> {
        self.common_type(context, operation, arguments, true)
    }

    // Shared ordered selection relations; comparison validation belongs only
    // to GREATEST/LEAST. ARRAY lifts the selected type into its result below.
    fn common_type(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
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
            let domain = InputTypeConstraint::exact(&crate::sem::PgBuiltInType::text());
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
            choices.extend(context.cast_sources(&target, false));
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
                crate::sem::PgBuiltInType::text()
            } else {
                source.clone()
            };
            let ordering = if comparison {
                MinMaxExpr::ordered(context, &target)
            } else {
                Some(true)
            };
            if ordering.is_none() {
                context.report_analysis_problem(crate::sem::AnalysisProblem::min_max(
                    crate::sem::problem::MinMaxProblem::missing_comparison_facts(&target),
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
            context.report_analysis_problem(crate::sem::AnalysisProblem::min_max(
                crate::sem::problem::MinMaxProblem::not_ordered(&target),
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
        context: &mut impl crate::sem::ParseContext,
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
            self.graph.project(
                element,
                result,
                crate::sem::parameter_constraints::TypeProjection::construct_array(),
            );
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

    pub(crate) fn retain_completed_outputs(&mut self) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.outputs = self.completed.clone();
        }
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
}
impl Signature {
    pub(crate) fn new(inputs: &[TypeReference], result: &TypeReference) -> Self {
        Self {
            inputs: inputs.to_vec(),
            result: result.clone(),
            declared_inputs: None,
        }
    }
    pub(crate) fn new_function(
        inputs: &[TypeReference],
        declared_inputs: &[TypeReference],
        result: &TypeReference,
    ) -> Option<Self> {
        if inputs.len() != declared_inputs.len() {
            return None;
        }
        let mut signature = Self::new(inputs, result);
        signature.declared_inputs = Some(declared_inputs.to_vec());
        if signature.is_concrete_for_arity(inputs.len()) {
            Some(signature)
        } else {
            None
        }
    }

    fn is_concrete_for_arity(&self, arity: usize) -> bool {
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

    fn source_types(
        &self,
        catalog: &impl BaseParseContext,
        index: usize,
    ) -> std::collections::HashSet<TypeReference> {
        catalog.cast_sources(&self.inputs[index], false)
    }

    fn cast_score(
        &self,
        catalog: &impl BaseParseContext,
        source: &TypeReference,
        index: usize,
    ) -> Option<OverloadRank> {
        if let Some(declared_inputs) = &self.declared_inputs {
            OverloadRank::function_argument(
                catalog,
                Some(source),
                Some(&self.inputs[index]),
                Some(&declared_inputs[index]),
            )
        } else {
            OverloadRank::argument(catalog, Some(source), Some(&self.inputs[index]))
        }
    }

    fn result_alternative(&self, inputs: &[InputTypeConstraint]) -> SignatureAlternative {
        let mut domains = inputs.to_vec();
        domains.push(InputTypeConstraint::exact(&self.result));
        SignatureAlternative::new(&domains)
    }

    // Partition source types by their cast scores against all candidates. Types
    // within one partition have identical overload behavior, so the solver can
    // keep ranges without enumerating every concrete input tuple.
    fn alternatives(
        signatures: &[Self],
        catalog: &impl BaseParseContext,
        inputs: &[InputTypeConstraint],
    ) -> Vec<SignatureAlternative> {
        let signatures: Vec<_> = signatures
            .iter()
            .filter(|signature| signature.is_concrete_for_arity(inputs.len()))
            .collect();
        let choices = SourcePartition::argument_choices(&signatures, catalog, inputs);
        let mut alternatives = Vec::new();
        Self::combine(
            &signatures,
            &choices,
            0,
            &[],
            &OverloadScores::initial(signatures.len()),
            &mut alternatives,
        );
        alternatives
    }

    fn combine(
        signatures: &[&Self],
        choices: &[Vec<SourcePartition>],
        index: usize,
        domains: &[InputTypeConstraint],
        scores: &OverloadScores,
        alternatives: &mut Vec<SignatureAlternative>,
    ) {
        if index == choices.len() {
            let Some(candidate) = scores.unique_best() else {
                return;
            };
            alternatives.push(signatures[candidate].result_alternative(domains));
            return;
        }
        for choice in &choices[index] {
            let next_scores = choice.accumulate_scores(scores);
            if next_scores.has_no_candidates() {
                continue;
            }
            let domains = choice.extend_domains(domains);
            crate::sem::with_stack(|| {
                Self::combine(
                    signatures,
                    choices,
                    index + 1,
                    &domains,
                    &next_scores,
                    alternatives,
                )
            });
        }
    }
}

struct SourcePartition {
    scores: OverloadScores,
    domain: InputTypeConstraint,
}

impl SourcePartition {
    // Sources in one partition have identical cast scores for every overload.
    fn argument_choices(
        signatures: &[&Signature],
        catalog: &impl BaseParseContext,
        inputs: &[InputTypeConstraint],
    ) -> Vec<Vec<Self>> {
        let mut choices = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let mut sources = std::collections::HashSet::new();
            for signature in signatures {
                sources.extend(signature.source_types(catalog, index));
            }
            let mut groups = BTreeMap::<Vec<Option<OverloadRank>>, Vec<TypeReference>>::new();
            for source in sources {
                if !input.allows(&source) {
                    continue;
                }
                let scores = signatures
                    .iter()
                    .map(|signature| signature.cast_score(catalog, &source, index))
                    .collect();
                groups.entry(scores).or_default().push(source);
            }
            choices.push(
                groups
                    .into_iter()
                    .map(|(scores, types)| Self::new(&scores, &types))
                    .collect::<Vec<_>>(),
            );
        }
        choices
    }

    fn new(scores: &[Option<OverloadRank>], types: &[TypeReference]) -> Self {
        Self {
            scores: OverloadScores::new(scores),
            domain: InputTypeConstraint::types(types.to_vec()),
        }
    }

    fn accumulate_scores(&self, previous: &OverloadScores) -> OverloadScores {
        self.scores.add(previous)
    }

    fn extend_domains(&self, previous: &[InputTypeConstraint]) -> Vec<InputTypeConstraint> {
        let mut domains = previous.to_vec();
        domains.push(self.domain.clone());
        domains
    }
}

struct OverloadScores(Vec<Option<OverloadRank>>);

impl OverloadScores {
    fn new(scores: &[Option<OverloadRank>]) -> Self {
        Self(scores.to_vec())
    }

    fn initial(count: usize) -> Self {
        Self(vec![Some(OverloadRank::default()); count])
    }

    fn add(&self, previous: &Self) -> Self {
        Self(
            previous
                .0
                .iter()
                .zip(&self.0)
                .map(|(left, right)| Some((*left)? + (*right)?))
                .collect(),
        )
    }

    fn has_no_candidates(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    fn unique_best(&self) -> Option<usize> {
        let best = self.0.iter().flatten().max()?;
        let mut candidates = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, score)| score.as_ref() == Some(best));
        let (candidate, _) = candidates.next()?;
        // Tied best overloads are ambiguous for this concrete source assignment.
        if candidates.next().is_some() {
            return None;
        }
        Some(candidate)
    }
}

struct Collector<'a, C: crate::sem::ParseContext> {
    dml_depth: usize,
    parameters: ParameterResolution,
    context: &'a mut C,
}

impl<C: crate::sem::ParseContext> crate::sem::traversal::SemanticVisitor for Collector<'_, C> {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.collect_constraints(&mut self.parameters, self.context)
    }
    fn assignment(
        &mut self,
        value: &mut SemScalarExpr,
        target: &crate::sem::InsertTarget,
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
            if let Some(node) = value.constraint_node() {
                self.parameters.graph.restrict(
                    node,
                    &InputTypeConstraint::types(self.context.assignment_sources(ty)),
                );
            } else if let Some(source) = value.get_type()
                && &source != ty
                && self.context.get_assignment_cast(&source, ty).is_none()
            {
                self.context.report_analysis_problem(
                    crate::sem::AnalysisProblem::parameter_constraint(&format!(
                        "cannot assign {source:?} to {ty:?}"
                    )),
                )?;
            }
            if value.get_type().as_ref() != Some(ty) {
                *value = SemScalarExpr::wrap_implicit_cast(ty, value);
            }
        }
        Ok(())
    }
    fn dml_returning(
        &mut self,
        columns: &mut crate::sem::ResultColumnCollection,
    ) -> Result<(), AnalysisError> {
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
                    self.context
                        .assignment_sources(&crate::sem::PgBuiltInType::int4()),
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
    fn enter_query(&mut self) {
        self.parameters.enter_query();
    }
    fn leave_query(&mut self) {
        if self.dml_depth > 0 && self.context.dialect() == Dialect::postgres() {
            self.parameters.default_query_outputs(self.context);
        }
        self.parameters.leave_query();
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
                && expression.is_unconstrained_parameter(),
        );
    }
    fn predicate(&mut self, value: &SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres() {
            self.parameters.require(
                self.context,
                value,
                &crate::sem::PgBuiltInType::bool(),
                false,
            );
        }
    }
    fn order_operator(
        &mut self,
        value: &SemScalarExpr,
        operator: &crate::sem::ResolvedOperator,
    ) -> Result<(), AnalysisError> {
        operator.infer(&mut self.parameters, self.context, Some(value), Some(value))?;
        Ok(())
    }
    fn row_count(&mut self, value: &SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres() {
            self.parameters.require(
                self.context,
                value,
                &crate::sem::PgBuiltInType::int8(),
                false,
            );
        }
    }
    fn window_definition(&mut self, window: &crate::sem::QueryWindow) {
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
        order: &[crate::sem::QueryOrder],
        reference: Option<&crate::sem::QueryName>,
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
    fn retain_set_outputs(&mut self) {
        self.parameters.retain_completed_outputs();
    }
}

struct Finalizer<'a, C: crate::sem::ParseContext> {
    constraints: &'a ParameterConstraints,
    context: &'a C,
}
impl<C: crate::sem::ParseContext> crate::sem::traversal::SemanticVisitor for Finalizer<'_, C> {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.apply_constraints(self.constraints);
        Ok(())
    }
    fn output(&mut self, _name: &str, expression: &mut SemScalarExpr) {
        if self.context.dialect() == Dialect::postgres() && expression.is_unconstrained_parameter()
        {
            *expression =
                SemScalarExpr::wrap_implicit_cast(&crate::sem::PgBuiltInType::text(), expression);
        }
    }
}

impl ParameterResolution {
    pub(crate) fn infer_query(
        query: &mut crate::sem::SelectStatement,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            dml_depth: 0,
            parameters: Self::default(),
            context,
        };
        query.walk(&mut collector)?;
        collector.parameters.solve(collector.context)?;
        let constraints = collector.parameters.constraints();
        query.walk(&mut Finalizer {
            constraints: &constraints,
            context: collector.context,
        })?;
        Ok(constraints)
    }
    pub(crate) fn infer_insert(
        insert: &mut crate::sem::InsertStatement,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            dml_depth: 0,
            parameters: Self::default(),
            context,
        };
        insert.walk(&mut collector)?;
        collector.parameters.solve(collector.context)?;
        let constraints = collector.parameters.constraints();
        insert.walk(&mut Finalizer {
            constraints: &constraints,
            context: collector.context,
        })?;
        Ok(constraints)
    }
    pub(crate) fn infer_update(
        update: &mut crate::sem::UpdateStatement,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            dml_depth: 0,
            parameters: Self::default(),
            context,
        };
        update.walk(&mut collector)?;
        collector.parameters.solve(collector.context)?;
        let constraints = collector.parameters.constraints();
        update.walk(&mut Finalizer {
            constraints: &constraints,
            context: collector.context,
        })?;
        Ok(constraints)
    }
    pub(crate) fn infer_delete(
        delete: &mut crate::sem::DeleteStatement,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            dml_depth: 0,
            parameters: Self::default(),
            context,
        };
        delete.walk(&mut collector)?;
        collector.parameters.solve(collector.context)?;
        let constraints = collector.parameters.constraints();
        delete.walk(&mut Finalizer {
            constraints: &constraints,
            context: collector.context,
        })?;
        Ok(constraints)
    }
    pub(crate) fn infer_merge(
        merge: &mut crate::sem::MergeStatement,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            dml_depth: 0,
            parameters: Self::default(),
            context,
        };
        merge.walk(&mut collector)?;
        collector.parameters.solve(collector.context)?;
        let constraints = collector.parameters.constraints();
        merge.walk(&mut Finalizer {
            constraints: &constraints,
            context: collector.context,
        })?;
        Ok(constraints)
    }
    pub(crate) fn infer_expression(
        expression: &mut SemScalarExpr,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
            dml_depth: 0,
            parameters: Self::default(),
            context,
        };
        expression.walk(&mut collector)?;
        collector.parameters.solve(collector.context)?;
        let constraints = collector.parameters.constraints();
        expression.walk(&mut Finalizer {
            constraints: &constraints,
            context: collector.context,
        })?;
        Ok(constraints)
    }

    pub(crate) fn operator(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
        name: &crate::sem::OperatorName,
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
        if let Some(operator) = context.get_operator_op(name, left.is_some(), right.is_some()) {
            let signatures = operator.parameter_signatures();
            return Ok(Some(self.call(
                context,
                &format!("{name:?}"),
                &arguments,
                &signatures,
            )?));
        }
        Ok(Some(self.operation(
            &format!("{name:?}"),
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
