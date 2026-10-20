//! SQL-derived parameter domains and correlated overload alternatives.

mod structural;

pub(crate) use self::structural::TypeProjection;

use self::structural::StructuralConstraint;

use crate::sem::{ParameterConstraintProblem, TypeReference};
use std::collections::BTreeMap;

/// Admissible SQL types. An unconstrained domain is `any`, not inference failure.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InputTypeConstraint(Option<Vec<TypeReference>>);

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

    pub(crate) fn single_type(&self) -> Option<TypeReference> {
        match self.0.as_deref() {
            Some([ty]) => Some(ty.clone()),
            _ => None,
        }
    }

    fn intersect(&mut self, other: &Self) -> bool {
        let before = self.clone();
        match (&mut self.0, &other.0) {
            (Some(types), Some(other)) => types.retain(|ty| other.contains(ty)),
            (None, Some(types)) => self.0 = Some(types.clone()),
            _ => {}
        }
        *self != before
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

    fn overlaps(&self, other: &Self) -> bool {
        let mut domain = self.clone();
        domain.intersect(other);
        !domain.is_empty()
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
    /// Admissible input domain, independent of parameter hints and Rust mapping.
    pub const fn constraint(&self) -> &InputTypeConstraint {
        &self.constraint
    }
}

/// Statement-wide input constraints, including relationships between overloads.
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

#[derive(Clone, Debug)]
pub(crate) struct SignatureAlternative(Vec<InputTypeConstraint>);

impl SignatureAlternative {
    pub(crate) fn new(domains: &[InputTypeConstraint]) -> Self {
        Self(domains.to_vec())
    }
    // Repeated operands share one node, so their requirements must intersect
    // before comparing this alternative with the current graph domains.
    fn compatible_with(&self, nodes: &[usize], domains: &[InputTypeConstraint]) -> bool {
        let mut requirements = BTreeMap::<usize, InputTypeConstraint>::new();
        for (node, required) in nodes.iter().zip(&self.0) {
            requirements
                .entry(*node)
                .or_insert_with(InputTypeConstraint::any)
                .intersect(required);
        }
        requirements
            .into_iter()
            .all(|(node, required)| domains[node].overlaps(&required))
    }

    fn include_domain(&self, index: usize, allowed: &mut InputTypeConstraint) {
        allowed.union(&self.0[index]);
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

    fn retain_compatible(&mut self, domains: &[InputTypeConstraint]) -> bool {
        self.alternatives
            .retain(|alternative| alternative.compatible_with(&self.nodes, domains));
        !self.alternatives.is_empty()
    }

    fn propagate_domains(&self, domains: &mut [InputTypeConstraint]) -> bool {
        let mut changed = false;
        for (index, node) in self.nodes.iter().enumerate() {
            let mut allowed = InputTypeConstraint::types([]);
            for alternative in &self.alternatives {
                alternative.include_domain(index, &mut allowed);
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
}

impl ConstraintGraph {
    fn parameter_constraints(&self) -> Vec<ParameterConstraint> {
        self.parameters
            .iter()
            .map(|(position, node)| ParameterConstraint::new(*position, self.domain(*node)))
            .collect()
    }

    fn accepts(&self, types: &BTreeMap<u32, TypeReference>) -> bool {
        let mut graph = self.clone();
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
        graph.satisfiable()
    }

    pub(crate) fn node(&mut self, domain: &InputTypeConstraint) -> usize {
        let node = self.domains.len();
        self.domains.push(domain.clone());
        node
    }

    pub(crate) fn parameter(&mut self, position: u32) -> usize {
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
        self.equalities.push((left, right));
    }

    pub(crate) fn restrict(&mut self, node: usize, domain: &InputTypeConstraint) {
        self.requirements
            .push(format!("input node {node}: {domain:?}"));
        self.domains[node].intersect(domain);
    }

    pub(crate) fn overload(
        &mut self,
        nodes: &[usize],
        alternatives: &[SignatureAlternative],
        context: &str,
    ) {
        self.overloads
            .push(OverloadConstraint::new(nodes, alternatives, context));
    }

    pub(crate) fn project(&mut self, source: usize, result: usize, projection: TypeProjection) {
        self.structural
            .push(StructuralConstraint::new(source, result, projection));
    }

    pub(crate) fn prepare_projections(&mut self, context: &impl crate::sem::ParseContext) {
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
    }

    pub(crate) fn report_projection_problems(
        &self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), crate::sem::AnalysisError> {
        for relation in &self.structural {
            relation.report_problem(&self.domains, context)?;
        }
        Ok(())
    }

    fn propagate(&mut self) -> bool {
        loop {
            if self.domains.iter().any(InputTypeConstraint::is_empty) {
                return false;
            }
            let mut changed = false;
            for (left, right) in &self.equalities {
                let domain = self.domains[*left].clone();
                changed |= self.domains[*right].intersect(&domain);
                let domain = self.domains[*right].clone();
                changed |= self.domains[*left].intersect(&domain);
            }
            for overload in &mut self.overloads {
                if !overload.retain_compatible(&self.domains) {
                    return false;
                }
                changed |= overload.propagate_domains(&mut self.domains);
            }

            for relation in &self.structural {
                changed |= relation.propagate(&mut self.domains);
            }

            if !changed {
                return true;
            }
        }
    }

    fn satisfiable(&mut self) -> bool {
        if self.parameters.contains_key(&0) {
            return false;
        }
        if !self.propagate() {
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
            if crate::sem::with_stack(|| branch.satisfiable()) {
                return true;
            }
        }
        false
    }

    pub(crate) fn solve(&mut self) -> Result<(), ParameterConstraintProblem> {
        if self.parameters.contains_key(&0) {
            return Err(ParameterConstraintProblem::new(
                "parameter positions start at one",
            ));
        }
        if !self.satisfiable() {
            return Err(self.conflict());
        }
        // Remove globally impossible alternatives, not just locally inconsistent
        // ones. Retained alternatives stay correlated in the public result.
        for index in 0..self.overloads.len() {
            let mut retained = Vec::new();
            for alternative in self.overloads[index].alternatives.clone() {
                let mut branch = self.clone();
                branch.overloads[index].alternatives = vec![alternative.clone()];
                if branch.satisfiable() {
                    retained.push(alternative);
                }
            }
            self.overloads[index].alternatives = retained;
        }
        if !self.propagate() {
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
                if branch.satisfiable() {
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
    use super::{ConstraintGraph, InputTypeConstraint, ParameterConstraints, SignatureAlternative};
    use crate::sem::PgBuiltInType;
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
