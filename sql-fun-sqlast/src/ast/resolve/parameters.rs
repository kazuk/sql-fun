//! Analysis-local graph building and replay. Source AST and catalog models stay intact.
use crate::ast::Expr;
use crate::sem::{
    AnalysisError, Catalog, ConstrainedExpr, ConstraintGraph, InputTypeConstraint,
    ParameterConstraintProblem, ParameterConstraints, SemScalarExpr, SignatureAlternative,
    TypeReference,
};
use std::collections::{BTreeMap, HashMap};

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
            &[],
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
        let outputs = name
            .and_then(|name| self.ctes.get(name))
            .cloned()
            .unwrap_or_default();
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
    expressions: HashMap<usize, usize>,
    active_expression: Option<usize>,
    scopes: Vec<QueryScope>,
    completed: Outputs,
    replay: bool,
}

impl ParameterResolution {
    pub(crate) fn constraints(&self) -> ParameterConstraints {
        let mut constraints = ParameterConstraints::from_graph(&self.graph);
        constraints.set_outputs(&self.completed.constraints(&self.graph));
        constraints
    }

    pub(crate) fn is_any(&self, value: &SemScalarExpr) -> bool {
        value
            .constraint_node()
            .is_some_and(|node| self.graph.domain(node).is_any())
    }

    pub(crate) fn solve(&mut self) -> Result<bool, ParameterConstraintProblem> {
        if !self.graph.has_parameters() {
            return Ok(false);
        }
        self.graph.solve()?;
        self.replay = true;
        self.scopes.clear();
        self.completed = Outputs::default();
        Ok(true)
    }

    // Replay rebuilds the whole semantic tree, including queries nested in an
    // expression, while retaining the graph nodes from the collection pass.
    pub(crate) fn enter_expression(&mut self, expression: &Expr) -> Option<usize> {
        let previous = self.active_expression;
        self.active_expression = self
            .expressions
            .get(&(std::ptr::from_ref(expression) as usize))
            .copied();
        previous
    }

    pub(crate) const fn leave_expression(&mut self, previous: Option<usize>) {
        self.active_expression = previous;
    }

    pub(crate) fn remember(&mut self, expression: &Expr, value: &SemScalarExpr) {
        if let Some(node) = value.constraint_node() {
            self.expressions
                .insert(std::ptr::from_ref(expression) as usize, node);
        }
    }

    pub(crate) fn parameter(&mut self, position: u32) -> SemScalarExpr {
        let node = self.graph.parameter(position);
        SemScalarExpr::parameter(position, node, self.graph.domain(node))
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
        let node = if self.replay {
            self.active_expression
                .expect("replayed operation has a collected node")
        } else {
            self.graph.node(domain)
        };
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
        catalog: &dyn Catalog,
        value: &SemScalarExpr,
        target: &TypeReference,
        explicit: bool,
    ) {
        if !self.replay
            && let Some(node) = value.constraint_node()
        {
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(catalog.cast_sources(target, explicit)),
            );
        }
    }

    pub(crate) fn call(
        &mut self,
        catalog: &dyn Catalog,
        name: &str,
        arguments: &[SemScalarExpr],
        signatures: &[Signature],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let result = self.operation(name, arguments, &InputTypeConstraint::any(), None);
        if !self.replay {
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
        }
        Ok(result)
    }

    pub(crate) fn scalar_result(&mut self, value: &SemScalarExpr) -> Option<SemScalarExpr> {
        let source = self.completed.scalar_node()?;
        let result = self.operation(
            "scalar subquery",
            std::slice::from_ref(value),
            &InputTypeConstraint::any(),
            value.is_not_null(),
        );
        if !self.replay {
            self.graph
                .equal(source, result.constraint_node().expect("operation node"));
        }
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
        let (alias, _) = original.constraint_binding()?;
        for scope in self.scopes.iter().rev() {
            if scope.has_source(alias) {
                return scope.column(original, &self.graph);
            }
        }
        None
    }
}

pub(crate) struct Signature {
    inputs: Vec<TypeReference>,
    result: TypeReference,
}
impl Signature {
    pub(crate) fn new(inputs: &[TypeReference], result: &TypeReference) -> Self {
        Self {
            inputs: inputs.to_vec(),
            result: result.clone(),
        }
    }
    pub(crate) fn new_concrete(inputs: &[TypeReference], result: &TypeReference) -> Option<Self> {
        let signature = Self::new(inputs, result);
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
        catalog: &dyn Catalog,
        index: usize,
    ) -> std::collections::HashSet<TypeReference> {
        catalog.cast_sources(&self.inputs[index], false)
    }

    fn cast_score(
        &self,
        catalog: &dyn Catalog,
        source: &TypeReference,
        index: usize,
    ) -> Option<i32> {
        Some(
            catalog
                .get_implicit_cast(source, &self.inputs[index])?
                .context()
                .overload_priority_score(),
        )
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
        catalog: &dyn Catalog,
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
        catalog: &dyn Catalog,
        inputs: &[InputTypeConstraint],
    ) -> Vec<Vec<Self>> {
        let mut choices = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let mut sources = std::collections::HashSet::new();
            for signature in signatures {
                sources.extend(signature.source_types(catalog, index));
            }
            let mut groups = BTreeMap::<Vec<Option<i32>>, Vec<TypeReference>>::new();
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

    fn new(scores: &[Option<i32>], types: &[TypeReference]) -> Self {
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

struct OverloadScores(Vec<Option<i32>>);

impl OverloadScores {
    fn new(scores: &[Option<i32>]) -> Self {
        Self(scores.to_vec())
    }

    fn initial(count: usize) -> Self {
        Self(vec![Some(0); count])
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
