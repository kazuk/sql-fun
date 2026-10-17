//! Parameter inference over the name-resolved semantic tree.
use crate::ast::Dialect;
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
        if let Err(problem) = self.graph.solve() {
            crate::sem::ParseContext::report_analysis_problem(
                context,
                crate::sem::AnalysisProblem::parameter_constraint(problem.message()),
            )?;
        }
        Ok(())
    }

    pub(crate) fn parameter(&mut self, position: u32) -> SemScalarExpr {
        let node = self.graph.parameter(position);
        SemScalarExpr::parameter(position, node, self.graph.domain(node))
    }

    pub(crate) fn unify_case_types(&mut self, _case: &crate::sem::CaseExpr) {
        todo!("parameter-dependent CASE type unification");
    }
    pub(crate) fn unify_coalesce_types(&mut self, _coalesce: &crate::sem::CoalesceExpr) {
        todo!("parameter-dependent COALESCE type unification");
    }
    pub(crate) fn constrain_indirection(&mut self, _indirection: &crate::sem::IndirectionExpr) {
        todo!("parameter-dependent indirection type constraints");
    }
    pub(crate) fn sqlite_operator(
        &mut self,
        _name: &crate::sem::OperatorName,
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

struct Collector<'a, C: crate::sem::ParseContext> {
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
        if self.context.dialect() == Dialect::postgres()
            && let Some(ty) = target.assignment_type()
        {
            if let Some(node) = value.constraint_node() {
                self.parameters.graph.restrict(
                    node,
                    &InputTypeConstraint::types(self.context.assignment_sources(&ty)),
                );
            } else if let Some(source) = value.get_type()
                && source != ty
                && !self.context.get_assignment_cast(&source, &ty).is_some()
            {
                self.context
                    .report_analysis_problem(crate::sem::AnalysisProblem::insert(
                        crate::sem::problem::InsertProblemKind::assignment_type(),
                        &format!("cannot assign {source:?} to {ty:?}"),
                    ))?;
            }
            if value.get_type().as_ref() != Some(&ty) {
                *value = SemScalarExpr::wrap_implicit_cast(&ty, value);
            }
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
    fn enter_query(&mut self) {
        self.parameters.enter_query();
    }
    fn leave_query(&mut self) {
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
    pub(crate) fn infer_expression(
        expression: &mut SemScalarExpr,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<ParameterConstraints, AnalysisError> {
        #[cfg(test)]
        INFERENCE_RUNS.with(|runs| runs.set(runs.get() + 1));
        let mut collector = Collector {
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
        if !left.is_some_and(|arg| arg.constraint_node().is_some())
            && !right.is_some_and(|arg| arg.constraint_node().is_some())
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
