//! SQL-derived parameter domains and correlated overload alternatives.

mod call;
mod common_type;
mod memo;
mod structural;

pub(crate) use self::call::array_cast_ranks;
pub(crate) use self::memo::ConstraintMemo;
pub(crate) use self::structural::TypeProjection;

use crate::{
    CacheContext,
    sem::{
        AnalysisError, AnalysisProblem, ParseContext, parameter_resolution::Signature,
        problem::MinMaxProblem, with_stack,
    },
};

use self::common_type::{CommonTypeConstraint, CommonTypeFacts};
use std::sync::Arc;

use self::structural::StructuralConstraint;

use crate::sem::{ParameterConstraintProblem, TypeReference};
use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

fn default_types() -> [TypeReference; 7] {
    [
        "bool", "int4", "int8", "float4", "float8", "text", "numeric",
    ]
    .map(TypeReference::from_master_type_name)
}

/// Admissible SQL types. An unconstrained domain is `any`, not inference failure.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InputTypeConstraint(Option<Vec<TypeReference>>);

impl std::fmt::Display for InputTypeConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            None => f.write_str("any SQL type"),
            Some(types) if types.is_empty() => f.write_str("an unsatisfiable type requirement"),
            Some(types) => {
                f.write_str("one of (")?;
                for (index, ty) in types.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl InputTypeConstraint {
    /// No restriction on the input type.
    pub const fn any() -> Self {
        Self(None)
    }

    pub(crate) fn types(types: impl IntoIterator<Item = TypeReference>) -> Self {
        let mut types: Vec<_> = types.into_iter().collect();
        types.sort_by_cached_key(|ty| format!("{ty:?}"));
        types.dedup();
        Self(Some(types))
    }

    pub(crate) fn exact(ty: &TypeReference) -> Self {
        Self::types([ty.clone()])
    }

    /// Whether every input type is admissible.
    pub const fn is_any(&self) -> bool {
        self.0.is_none()
    }

    /// Finite admissible types, or `None` for `any`.
    pub fn admissible_types(&self) -> Option<&[TypeReference]> {
        self.0.as_deref()
    }

    /// Test membership in this domain. Correlated uses also need
    /// [`ParameterConstraints::allows`].
    pub fn allows(&self, ty: &TypeReference) -> bool {
        self.0.as_ref().is_none_or(|types| types.contains(ty))
    }

    /// A definite SQL type only when exactly one candidate remains.
    #[must_use]
    pub fn single_type(&self) -> Option<TypeReference> {
        match self.0.as_deref() {
            Some([ty]) => Some(ty.clone()),
            _ => None,
        }
    }

    fn intersect(&mut self, other: &Self) -> bool {
        match (&mut self.0, &other.0) {
            (Some(types), Some(other)) => {
                let before = types.len();
                types.retain(|ty| other.contains(ty));
                types.len() != before
            }
            (None, Some(types)) => {
                self.0 = Some(types.clone());
                true
            }
            _ => false,
        }
    }

    fn exclude(&mut self, excluded: &[TypeReference]) {
        if let Some(types) = &mut self.0 {
            types.retain(|ty| !excluded.contains(ty));
        }
    }

    fn retain_projection(&mut self, projection: &TypeProjection, result: &Self) {
        if let Some(types) = &mut self.0 {
            types.retain(|source| {
                projection
                    .project(source)
                    .is_some_and(|ty| result.allows(&ty))
            });
        }
    }

    fn projected(&self, projection: &TypeProjection) -> Self {
        match &self.0 {
            Some(types) => Self::types(types.iter().filter_map(|ty| projection.project(ty))),
            None => Self::any(),
        }
    }

    fn preimage(&self, projection: &TypeProjection) -> Self {
        match &self.0 {
            Some(types) => Self::types(types.iter().filter_map(|ty| projection.inverse(ty))),
            None => Self::any(),
        }
    }

    fn is_empty(&self) -> bool {
        self.0.as_ref().is_some_and(Vec::is_empty)
    }

    fn union(&mut self, other: &Self) {
        match (&mut self.0, &other.0) {
            (Some(types), Some(other)) => {
                for ty in other {
                    if !types.contains(ty) {
                        types.push(ty.clone());
                    }
                }
            }
            (_, None) => self.0 = None,
            _ => {}
        }
    }
}

/// One numbered input's SQL-derived constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterConstraint {
    position: u32,
    constraint: InputTypeConstraint,
}

impl ParameterConstraint {
    fn new(position: u32, constraint: &InputTypeConstraint) -> Self {
        Self {
            position,
            constraint: constraint.clone(),
        }
    }

    /// Original one-based SQL parameter position; gaps are preserved.
    pub const fn position(&self) -> u32 {
        self.position
    }
    /// Assigned or inferred input candidates. Unhinted overloads can expose
    /// bounded defaults; use [`ParameterConstraints::allows`] to validate an
    /// explicit assignment against the complete correlated SQL constraints.
    pub const fn constraint(&self) -> &InputTypeConstraint {
        &self.constraint
    }
}

/// Statement-wide input constraints, including relationships between overloads.
///
/// Domains summarize SQL-derived input candidates, with bounded defaults for
/// unhinted overloads. [`Self::allows`] checks explicitly supplied types against
/// the complete SQL constraints. Analysis does not select the single parameter
/// signature chosen by PostgreSQL's untyped `PREPARE`.
///
/// A result column can have several SQL types when its output depends on the
/// explicit input types. For example, `ARRAY[$1,1]` admits an integer input with
/// an integer-array result and a bigint input with a bigint-array result.
/// Inspect [`Self::result_column`] rather than assuming one result type. A
/// single result column type may be unavailable even when these alternatives are
/// known.
///
/// [`Self::allows`] checks correlated assignments across parameter positions;
/// checking each parameter's domain independently can admit an incompatible
/// combination. Omitted positions remain unconstrained by the check, while
/// unknown positions are rejected. Inspection does not install hints, choose an
/// application binding, narrow the retained result domains or change executable
/// SQL.
///
#[derive(Clone, Debug, Default)]
pub struct ParameterConstraints {
    parameters: Vec<ParameterConstraint>,
    outputs: Vec<InputTypeConstraint>,
    graph: Option<ConstraintGraph>,
}

impl ParameterConstraints {
    pub(crate) fn node_domain(&self, node: usize) -> &InputTypeConstraint {
        self.graph.as_ref().expect("inferred graph").domain(node)
    }

    pub(crate) fn set_outputs(&mut self, outputs: &[InputTypeConstraint]) {
        self.outputs = outputs.to_vec();
    }

    /// SQL-derived domain of a zero-based result column. A domain may retain
    /// several result types when the input type determines an overload's output.
    pub fn result_column(&self, index: usize) -> Option<&InputTypeConstraint> {
        self.outputs.get(index)
    }

    /// Inspect inputs in ascending numbered position.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ParameterConstraint> {
        self.parameters.iter()
    }

    /// Find an original numbered input without filling gaps.
    pub fn parameter(&self, position: u32) -> Option<&ParameterConstraint> {
        self.parameters
            .iter()
            .find(|parameter| parameter.position == position)
    }

    /// Whether the supplied concrete types can coexist in at least one valid
    /// overload assignment. Omitted positions remain unconstrained by this check.
    /// Unknown positions are rejected. This is inspection, not an analysis hint.
    /// Composite definitions encountered during analysis are retained. To inspect
    /// a previously unknown composite, supply a dynamic reference owning its
    /// definition; a new concrete name alone cannot provide its fields.
    pub fn allows(&self, types: &BTreeMap<u32, TypeReference>) -> bool {
        match &self.graph {
            Some(graph) => graph.accepts(types),
            None => types.is_empty(),
        }
    }

    pub(crate) fn from_graph(graph: &ConstraintGraph) -> Self {
        Self {
            parameters: graph.parameter_constraints(),
            outputs: Vec::new(),
            graph: Some(graph.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignatureAlternative {
    // Immutable requirements are shared by search branches and memo keys.
    domains: Arc<[InputTypeConstraint]>,
    // Unrestricted variadic positions exclude source types that were ranked
    // separately against concrete overloads. Kept private to the solver.
    excluded: Arc<[Vec<TypeReference>]>,
    fingerprint: u64,
}

// Requirements are immutable, so their hash is computed once and cloned with
// the alternative. Equality still compares complete domains and exclusions.
impl Hash for SignatureAlternative {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fingerprint.hash(state);
    }
}

impl SignatureAlternative {
    pub(crate) fn new(domains: &[InputTypeConstraint]) -> Self {
        Self::excluding(domains, &[])
    }

    pub(crate) fn excluding(
        domains: &[InputTypeConstraint],
        excluded: &[Vec<TypeReference>],
    ) -> Self {
        let mut fingerprint = DefaultHasher::new();
        domains.hash(&mut fingerprint);
        excluded.hash(&mut fingerprint);
        Self {
            domains: Arc::from(domains),
            excluded: Arc::from(excluded),
            fingerprint: fingerprint.finish(),
        }
    }

    fn permits_result(&self, result: &InputTypeConstraint) -> bool {
        if let Some(domain) = self.domains.last()
            && let Some(types) = domain.admissible_types()
        {
            types.iter().any(|ty| result.allows(ty))
        } else {
            true
        }
    }

    fn remove_excluded(&self, index: usize, domain: &mut InputTypeConstraint) {
        if let Some(excluded) = self.excluded.get(index) {
            domain.exclude(excluded);
        }
    }

    // Repeated operands share one node, so their requirements must intersect
    // before comparing this alternative with the current graph domains.
    fn compatible_with(&self, nodes: &[usize], domains: &[InputTypeConstraint]) -> bool {
        let mut requirements = BTreeMap::<usize, InputTypeConstraint>::new();
        for (node, required) in nodes.iter().zip(self.domains.as_ref()) {
            requirements
                .entry(*node)
                .or_insert_with(InputTypeConstraint::any)
                .intersect(required);
        }
        for (index, node) in nodes.iter().enumerate() {
            let required = requirements.get_mut(node).expect("operand requirement");
            required.intersect(&domains[*node]);
            self.remove_excluded(index, required);
        }
        requirements.values().all(|required| !required.is_empty())
    }

    fn include_domain(
        &self,
        index: usize,
        current: &InputTypeConstraint,
        allowed: &mut InputTypeConstraint,
    ) {
        let mut domain = self.domains[index].clone();
        domain.intersect(current);
        self.remove_excluded(index, &mut domain);
        allowed.union(&domain);
    }
}

#[derive(Clone, Debug)]
struct OverloadConstraint {
    nodes: Vec<usize>,
    alternatives: Vec<SignatureAlternative>,
    context: String,
}

impl OverloadConstraint {
    fn new(nodes: &[usize], alternatives: &[SignatureAlternative], context: &str) -> Self {
        Self {
            nodes: nodes.to_vec(),
            alternatives: alternatives.to_vec(),
            context: context.to_owned(),
        }
    }

    fn retain_compatible(
        &mut self,
        domains: &[InputTypeConstraint],
        memo: &mut ConstraintMemo,
    ) -> bool {
        let operands = self.operand_domains(domains, memo);
        self.alternatives
            .retain(|alternative| memo.compatible(alternative, &operands, &self.nodes, domains));
        !self.alternatives.is_empty()
    }

    fn propagate_domains(&self, domains: &mut [InputTypeConstraint]) -> bool {
        let mut changed = false;
        for (index, node) in self.nodes.iter().enumerate() {
            let mut allowed = InputTypeConstraint::types([]);
            for alternative in &self.alternatives {
                alternative.include_domain(index, &domains[*node], &mut allowed);
            }
            changed |= domains[*node].intersect(&allowed);
        }
        changed
    }
}

/// Private processing representation; never stored in catalog/source models.
#[derive(Clone, Debug, Default)]
pub(crate) struct ConstraintGraph {
    domains: Vec<InputTypeConstraint>,
    parameters: BTreeMap<u32, usize>,
    overloads: Vec<OverloadConstraint>,
    equalities: Vec<(usize, usize)>,
    structural: Vec<StructuralConstraint>,
    requirements: Vec<String>,
    common_types: Vec<CommonTypeConstraint>,
    common_problems: Vec<MinMaxProblem>,
    common_facts: Option<Arc<CommonTypeFacts>>,
    calls: Vec<call::CallConstraint>,
    unsupported_inference: Vec<String>,
    inspection: Option<Arc<Self>>,
}

impl ConstraintGraph {
    // A subsequent collection pass must extend SQL constraints, never defaults
    // or alternatives eliminated by a previous inference attempt.
    fn resume_collection(&mut self) {
        if let Some(original) = self.inspection.take() {
            *self = (*original).clone();
        }
    }

    fn parameter_constraints(&self) -> Vec<ParameterConstraint> {
        self.parameters
            .iter()
            .map(|(position, node)| ParameterConstraint::new(*position, self.domain(*node)))
            .collect()
    }

    fn accepts(&self, types: &BTreeMap<u32, TypeReference>) -> bool {
        let mut graph = self.inspection.as_deref().unwrap_or(self).clone();
        for (position, ty) in types {
            let Some(node) = graph.parameters.get(position).copied() else {
                return false;
            };
            for relation in &mut graph.structural {
                relation.retain_owned(ty);
            }
            let supplied = if graph.domains[node].allows(ty) {
                ty.clone()
            } else {
                TypeProjection::identity(ty)
            };
            graph.domains[node].intersect(&InputTypeConstraint::exact(&supplied));
        }
        graph.satisfiable(&mut ConstraintMemo::default()) && graph.unsupported_inference.is_empty()
    }

    pub(crate) fn node(&mut self, domain: &InputTypeConstraint) -> usize {
        self.resume_collection();
        let node = self.domains.len();
        self.domains.push(domain.clone());
        node
    }

    pub(crate) fn parameter(&mut self, position: u32) -> usize {
        self.resume_collection();
        if let Some(node) = self.parameters.get(&position) {
            return *node;
        }
        let node = self.node(&InputTypeConstraint::any());
        self.parameters.insert(position, node);
        node
    }

    pub(crate) fn domain(&self, node: usize) -> &InputTypeConstraint {
        &self.domains[node]
    }

    pub(crate) fn equal(&mut self, left: usize, right: usize) {
        self.resume_collection();
        self.equalities.push((left, right));
    }

    pub(crate) fn restrict(&mut self, node: usize, domain: &InputTypeConstraint) {
        self.resume_collection();
        self.requirements
            .push(format!("expression requires {domain}"));
        self.domains[node].intersect(domain);
    }

    pub(crate) fn overload(
        &mut self,
        nodes: &[usize],
        alternatives: &[SignatureAlternative],
        context: &str,
    ) {
        self.resume_collection();
        self.overloads
            .push(OverloadConstraint::new(nodes, alternatives, context));
    }

    pub(crate) fn call(
        &mut self,
        context: &impl ParseContext,
        cache: &mut CacheContext,
        nodes: &[usize],
        signatures: &[Signature],
        operation: &str,
    ) {
        self.resume_collection();
        let inputs: Vec<_> = nodes[..nodes.len() - 1]
            .iter()
            .map(|node| self.domain(*node).clone())
            .collect();
        // Parameter nodes retain their semantic origin across repeated uses
        // and derived-column aliases; expression results have distinct nodes.
        let parameters: Vec<_> = nodes[..nodes.len() - 1]
            .iter()
            .map(|node| self.parameters.values().any(|parameter| parameter == node))
            .collect();
        self.calls.push(call::CallConstraint::prepare(
            context,
            cache,
            nodes,
            signatures,
            &inputs,
            operation,
            &parameters,
        ));
    }

    fn instantiate_calls(&mut self, defaults: bool) -> bool {
        let mut changed = false;
        let mut deferred = Vec::new();
        for relation in std::mem::take(&mut self.calls) {
            if relation.instantiate(self, defaults) {
                changed = true;
            } else {
                deferred.push(relation);
            }
        }
        self.calls = deferred;
        changed
    }

    pub(crate) fn common_type(
        &mut self,
        context: &impl ParseContext,
        inputs: &[usize],
        comparison: bool,
        operation: &str,
    ) -> usize {
        self.resume_collection();
        if self.common_facts.is_none() {
            self.common_facts = Some(Arc::new(CommonTypeFacts::prepare(context)));
        }
        let result = self.node(&InputTypeConstraint::any());
        self.common_types.push(CommonTypeConstraint::new(
            inputs, result, comparison, operation,
        ));
        result
    }

    pub(crate) fn project(&mut self, source: usize, result: usize, projection: TypeProjection) {
        self.resume_collection();
        self.structural
            .push(StructuralConstraint::new(source, result, projection));
    }

    pub(crate) fn prepare_projections(
        &mut self,
        context: &impl ParseContext,
        cache: &mut CacheContext,
    ) {
        self.resume_collection();
        if let Some(facts) = &mut self.common_facts {
            let facts = Arc::make_mut(facts);
            for ty in self
                .domains
                .iter()
                .filter_map(InputTypeConstraint::admissible_types)
                .flatten()
            {
                facts.retain(context, ty);
            }
        }
        let mut types: Vec<_> = self
            .domains
            .iter()
            .filter_map(InputTypeConstraint::admissible_types)
            .flatten()
            .cloned()
            .collect();
        let mut visited = Vec::new();
        while let Some(ty) = types.pop() {
            if visited.contains(&ty) {
                continue;
            }
            for relation in &mut self.structural {
                if let Some(result) = relation.prepare(context, &ty) {
                    types.push(result);
                }
            }
            visited.push(ty);
        }
        for relation in &mut self.calls {
            relation.retain(context, cache, visited.iter());
        }
    }

    pub(crate) fn report_inference_problems(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        for operation in &self.unsupported_inference {
            context.report_analysis_problem(
                AnalysisProblem::inference_candidate_space_unsupported(operation),
            )?;
        }
        Ok(())
    }

    pub(crate) fn report_projection_problems(
        &self,
        context: &mut impl ParseContext,
        memo: &mut ConstraintMemo,
    ) -> Result<(), AnalysisError> {
        // Inspect each relationship before it can discard an invalid source.
        // A clone preserves the original graph for solving and conflict reporting.
        let mut diagnostic_graph = self.inspection.as_deref().unwrap_or(self).clone();
        diagnostic_graph.propagate_with(false, memo, |relation, domains| {
            relation.report_problem(domains, context)
        })?;
        for problem in diagnostic_graph.common_problems {
            context.report_analysis_problem(AnalysisProblem::min_max(problem))?;
        }
        Ok(())
    }

    fn is_parameter(&self, node: usize) -> bool {
        self.parameters.values().any(|parameter| *parameter == node)
    }

    fn instantiate_common_types(&mut self, defaults: bool) -> bool {
        let Some(mut facts) = self.common_facts.take() else {
            return false;
        };
        let mut changed = false;
        let mut deferred = Vec::new();
        for relation in std::mem::take(&mut self.common_types) {
            if relation.instantiate(self, Arc::make_mut(&mut facts), defaults) {
                changed = true;
            } else {
                deferred.push(relation);
            }
        }
        self.common_types = deferred;
        self.common_facts = Some(facts);
        changed
    }

    fn propagate(&mut self, defaults: bool, memo: &mut ConstraintMemo) -> bool {
        self.propagate_with(
            defaults,
            memo,
            |_, _| Ok::<(), std::convert::Infallible>(()),
        )
        .expect("propagation without diagnostic delivery is infallible")
    }

    fn propagate_with<E>(
        &mut self,
        defaults: bool,
        memo: &mut ConstraintMemo,
        mut inspect: impl FnMut(&StructuralConstraint, &[InputTypeConstraint]) -> Result<(), E>,
    ) -> Result<bool, E> {
        loop {
            if self.domains.iter().any(InputTypeConstraint::is_empty) {
                return Ok(false);
            }
            let mut changed = false;
            for (left, right) in &self.equalities {
                let domain = self.domains[*left].clone();
                changed |= self.domains[*right].intersect(&domain);
                let domain = self.domains[*right].clone();
                changed |= self.domains[*left].intersect(&domain);
            }
            for overload in &mut self.overloads {
                if !overload.retain_compatible(&self.domains, memo) {
                    return Ok(false);
                }
                changed |= overload.propagate_domains(&mut self.domains);
            }

            for relation in &self.structural {
                inspect(relation, &self.domains)?;
                changed |= relation.propagate(&mut self.domains);
            }

            // Finish equality/structural propagation before materializing
            // finite common-type witnesses from the observed domains.
            if !changed {
                changed = self.instantiate_calls(defaults);
                if !changed {
                    changed = self.instantiate_common_types(defaults);
                }
                if !changed {
                    return Ok(true);
                }
            }
        }
    }

    fn satisfiable(&mut self, memo: &mut ConstraintMemo) -> bool {
        if self.parameters.contains_key(&0) {
            return false;
        }
        if !self.propagate(false, memo) {
            return false;
        }
        if (!self.calls.is_empty() || !self.common_types.is_empty()) && !self.propagate(true, memo)
        {
            return false;
        }
        let branch = self
            .overloads
            .iter()
            .enumerate()
            .filter(|(_, overload)| overload.alternatives.len() > 1)
            .min_by_key(|(_, overload)| overload.alternatives.len())
            .map(|(index, _)| index);
        let Some(index) = branch else {
            return true;
        };
        for alternative in self.overloads[index].alternatives.clone() {
            let mut branch = self.clone();
            branch.overloads[index].alternatives = vec![alternative];
            if with_stack(|| branch.satisfiable(memo)) {
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn solve(&mut self) -> Result<(), ParameterConstraintProblem> {
        self.solve_with(&mut ConstraintMemo::default())
    }

    pub(crate) fn solve_with(
        &mut self,
        memo: &mut ConstraintMemo,
    ) -> Result<(), ParameterConstraintProblem> {
        self.resume_collection();
        let original = if self.calls.is_empty() && self.common_types.is_empty() {
            None
        } else {
            Some(Arc::new(self.clone()))
        };
        let result = self.solve_domains(memo);
        self.inspection = original;
        result
    }

    fn solve_domains(
        &mut self,
        memo: &mut ConstraintMemo,
    ) -> Result<(), ParameterConstraintProblem> {
        if self.parameters.contains_key(&0) {
            return Err(ParameterConstraintProblem::new(
                "parameter positions start at one",
            ));
        }
        if !self.satisfiable(memo) {
            return Err(self.conflict());
        }
        if !self.unsupported_inference.is_empty() {
            return Ok(());
        }
        // Remove globally impossible alternatives, not just locally inconsistent
        // ones. Retained alternatives stay correlated in the public result.
        for index in 0..self.overloads.len() {
            let mut retained = Vec::new();
            for alternative in self.overloads[index].alternatives.clone() {
                let mut branch = self.clone();
                branch.overloads[index].alternatives = vec![alternative.clone()];
                if branch.satisfiable(memo) {
                    retained.push(alternative);
                }
            }
            self.overloads[index].alternatives = retained;
        }
        if !self.propagate(false, memo) {
            return Err(self.conflict());
        }
        for node in 0..self.domains.len() {
            let Some(types) = self.domains[node].admissible_types().map(<[_]>::to_vec) else {
                continue;
            };
            let mut retained = Vec::new();
            for ty in types {
                let mut branch = self.clone();
                branch.domains[node].intersect(&InputTypeConstraint::exact(&ty));
                if branch.satisfiable(memo) {
                    retained.push(ty);
                }
            }
            self.domains[node] = InputTypeConstraint::types(retained);
        }
        Ok(())
    }

    fn conflict(&self) -> ParameterConstraintProblem {
        let positions: Vec<_> = self
            .parameters
            .keys()
            .map(|position| format!("${position}"))
            .collect();
        let contexts: Vec<_> = self
            .overloads
            .iter()
            .map(|overload| overload.context.as_str())
            .collect();
        ParameterConstraintProblem::new(&format!(
            "incompatible constraints for {} in {}; required types: {}",
            positions.join(", "),
            contexts.join(", "),
            self.requirements.join("; ")
        ))
    }
}

#[cfg(test)]
mod test_constraints {
    use crate::sem::{
        PgBuiltInType,
        parameter_constraints::{
            ConstraintGraph, InputTypeConstraint, ParameterConstraints, SignatureAlternative,
        },
    };
    use std::collections::BTreeMap;

    #[test]
    fn zero_position_is_a_recoverable_constraint_problem() {
        let mut graph = ConstraintGraph::default();
        graph.parameter(0);
        assert!(
            graph
                .solve()
                .unwrap_err()
                .message()
                .contains("positions start at one")
        );
        assert!(!ParameterConstraints::from_graph(&graph).allows(&BTreeMap::new()));
    }

    #[test]
    fn overload_alternatives_remain_correlated() {
        let int = InputTypeConstraint::exact(&PgBuiltInType::int4());
        let text = InputTypeConstraint::exact(&PgBuiltInType::text());
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        graph.overload(
            &[left, right],
            &[
                SignatureAlternative::new(&[int.clone(), text.clone()]),
                SignatureAlternative::new(&[text, int]),
            ],
            "paired",
        );
        graph.solve().unwrap();
        let constraints = ParameterConstraints::from_graph(&graph);
        assert!(constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::text())
        ])));
        assert!(constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::text()),
            (2, PgBuiltInType::int4())
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::int4())
        ])));
    }

    #[test]
    fn repeated_operand_positions_must_share_one_type() {
        let int = InputTypeConstraint::exact(&PgBuiltInType::int4());
        let text = InputTypeConstraint::exact(&PgBuiltInType::text());
        let mut graph = ConstraintGraph::default();
        let input = graph.parameter(1);
        graph.overload(
            &[input, input],
            &[SignatureAlternative::new(&[int, text])],
            "same input",
        );
        assert!(graph.solve().is_err());
    }

    #[test]
    fn global_cycle_conflict_is_not_mistaken_for_local_compatibility() {
        let int = InputTypeConstraint::exact(&PgBuiltInType::int4());
        let text = InputTypeConstraint::exact(&PgBuiltInType::text());
        let mut graph = ConstraintGraph::default();
        let first = graph.parameter(1);
        let second = graph.parameter(2);
        let third = graph.parameter(3);
        let alternatives = [
            SignatureAlternative::new(&[int.clone(), text.clone()]),
            SignatureAlternative::new(&[text, int]),
        ];
        graph.overload(&[first, second], &alternatives, "first pair");
        graph.overload(&[second, third], &alternatives, "second pair");
        graph.overload(&[third, first], &alternatives, "third pair");
        assert!(graph.solve().is_err());
    }
}
