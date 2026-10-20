//! Owned common-type facts and relationships for initially unrestricted inputs.
//! These are inference data, not a second catalog or analysis environment.
use crate::sem::parameter_constraints::{
    ConstraintGraph, InputTypeConstraint, SignatureAlternative,
};
use crate::sem::{MinMaxExpr, ParseContext, PgBuiltInType, TypeReference};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
struct TypeFacts {
    base: TypeReference,
    category: Option<(char, bool)>,
    targets: HashSet<TypeReference>,
    ordered: Option<bool>,
}

/// Retain only the facts needed after the borrowed ParseContext is gone.
/// Catalog-defined nominal types are included; supplied owned definitions can
/// additionally supply identity, structural, category, and ordering evidence.
#[derive(Clone, Debug, Default)]
pub(super) struct CommonTypeFacts(HashMap<TypeReference, TypeFacts>);

impl CommonTypeFacts {
    pub(super) fn prepare(context: &impl ParseContext) -> Self {
        let mut facts = Self::default();
        for ty in context.type_references() {
            facts.retain(context, &ty);
        }
        facts
    }

    pub(super) fn retain(&mut self, context: &impl ParseContext, ty: &TypeReference) {
        if self.0.contains_key(ty) {
            return;
        }
        let canonical = if ty.is_dynamic() {
            ty.clone()
        } else {
            context
                .resolve_type_name(ty.full_name())
                .map(|name| TypeReference::concrete_type_ref(&name, ty.is_array() == Some(true)))
                .unwrap_or_else(|| ty.clone())
        };
        let base = Self::catalog_base_type(context, &canonical);
        let category = if let Ok(category) = MinMaxExpr::category(context, ty) {
            Some(category)
        } else {
            MinMaxExpr::category(context, &base).ok()
        };
        let targets = if base.is_dynamic() {
            HashSet::new()
        } else {
            context.get_implicit_castable(&base)
        };
        let facts = TypeFacts {
            base,
            category,
            targets,
            ordered: MinMaxExpr::ordered(context, &canonical),
        };
        self.0.insert(ty.clone(), facts.clone());
        self.0.insert(canonical, facts);
    }

    fn catalog_base_type(context: &impl ParseContext, ty: &TypeReference) -> TypeReference {
        if ty.is_array() == Some(true) {
            return ty.clone();
        }
        let mut base = ty.clone();
        let mut seen = HashSet::new();
        while !base.is_dynamic() && seen.insert(base.clone()) {
            let Some(definition) = context.get_type(base.full_name()) else {
                break;
            };
            let Some(next) = definition.domain_base() else {
                break;
            };
            base = next.clone();
        }
        base
    }

    fn identity(ty: &TypeReference) -> TypeReference {
        if ty.is_array() != Some(true)
            && let Some(definition) = ty.type_definition()
            && !definition.is_anonymous_record()
            && let Some(name) = definition.full_name()
        {
            return TypeReference::concrete_type_ref(name, false);
        }
        ty.clone()
    }

    fn base(&self, ty: &TypeReference) -> TypeReference {
        match self.0.get(&Self::identity(ty)) {
            Some(facts) => facts.base.clone(),
            None => match ty.type_definition() {
                Some(definition) => match definition.domain_base() {
                    Some(base) => base.clone(),
                    None => ty.clone(),
                },
                None => ty.clone(),
            },
        }
    }

    fn category(&self, ty: &TypeReference) -> Option<(char, bool)> {
        if ty.is_array() == Some(true) {
            return Some(('A', false));
        }
        if let Some(facts) = self.0.get(ty)
            && let Some(category) = facts.category
        {
            return Some(category);
        }
        Some((ty.type_definition()?.intrinsic_category()?.code(), false))
    }

    fn ordered(&self, ty: &TypeReference) -> Option<bool> {
        let base = self.base(ty);
        if base.is_array() == Some(true) {
            return self.ordered(&base.element_type());
        }
        if let Some(facts) = self.0.get(&base)
            && let Some(ordered) = facts.ordered
        {
            return Some(ordered);
        }
        base.type_definition()
            .filter(|definition| definition.has_structural_ordering())
            .map(|_| true)
    }

    fn coerce(&self, source: &TypeReference, target: &TypeReference) -> bool {
        if source == target || *source == MinMaxExpr::unknown() {
            return true;
        }
        let source = self.base(source);
        if Self::identity(&source) == Self::identity(target) {
            return true;
        }
        if source.is_array() == Some(true) && target.is_array() == Some(true) {
            return self.coerce(&source.element_type(), &target.element_type());
        }
        if self
            .0
            .get(&source)
            .is_some_and(|facts| facts.targets.contains(&Self::identity(target)))
        {
            return true;
        }
        source.implicit_match_owned(target)
    }

    fn select(&self, left: &TypeReference, right: &TypeReference) -> Option<TypeReference> {
        if left == right {
            return Some(left.clone());
        }
        let left = self.base(left);
        let right = self.base(right);
        if left == MinMaxExpr::unknown() {
            return Some(right);
        }
        if right == MinMaxExpr::unknown() {
            return Some(left);
        }
        if left == right {
            return Some(left);
        }
        MinMaxExpr::select_candidate(
            &left,
            &right,
            self.category(&left)?,
            self.category(&right)?,
            self.coerce(&left, &right),
            self.coerce(&right, &left),
        )
        .ok()
    }

    // Complete the connected conversion families of the observed types. This
    // supplies witnesses for omitted inputs without choosing one of them.
    fn choices(&self, seeds: impl Iterator<Item = TypeReference>) -> InputTypeConstraint {
        let mut types: HashSet<_> = seeds.filter(|ty| *ty != MinMaxExpr::unknown()).collect();
        let array_elements: Vec<_> = types
            .iter()
            .filter(|ty| ty.is_array() == Some(true))
            .map(TypeReference::element_type)
            .collect();
        if !array_elements.is_empty() {
            types.extend(
                self.choices(array_elements.into_iter())
                    .admissible_types()
                    .unwrap_or(&[])
                    .iter()
                    .map(TypeReference::array_type),
            );
        }
        loop {
            let before = types.len();
            for (source, facts) in &self.0 {
                if types.contains(source)
                    || types.contains(&facts.base)
                    || facts.targets.iter().any(|target| types.contains(target))
                {
                    types.insert(source.clone());
                    types.insert(facts.base.clone());
                    types.extend(facts.targets.iter().cloned());
                }
            }
            if types.len() == before {
                break;
            }
        }
        InputTypeConstraint::types(types)
    }
}

#[derive(Clone, Debug)]
pub(super) struct CommonTypeConstraint {
    inputs: Vec<usize>,
    result: usize,
    comparison: bool,
    operation: String,
}

impl CommonTypeConstraint {
    pub(super) fn new(inputs: &[usize], result: usize, comparison: bool, operation: &str) -> Self {
        Self {
            inputs: inputs.to_vec(),
            result,
            comparison,
            operation: operation.to_owned(),
        }
    }

    pub(super) fn instantiate(&self, graph: &mut ConstraintGraph, facts: &CommonTypeFacts) -> bool {
        let seeds = self
            .inputs
            .iter()
            .chain(std::iter::once(&self.result))
            .filter_map(|node| graph.domain(*node).admissible_types())
            .flatten()
            .cloned();
        let choices = facts.choices(seeds);
        if choices.is_empty() {
            return false;
        }
        for input in &self.inputs {
            if graph.domain(*input).is_any() {
                graph.restrict(*input, &choices);
            }
        }
        let mut candidate = self.inputs[0];
        for input in self.inputs.iter().skip(1) {
            let mut alternatives = Vec::new();
            let mut targets = Vec::new();
            for left in graph.domain(candidate).admissible_types().unwrap_or(&[]) {
                for right in graph.domain(*input).admissible_types().unwrap_or(&[]) {
                    if let Some(target) = facts.select(left, right) {
                        alternatives.push(SignatureAlternative::new(&[
                            InputTypeConstraint::exact(left),
                            InputTypeConstraint::exact(right),
                            InputTypeConstraint::exact(&target),
                        ]));
                        targets.push(target);
                    }
                }
            }
            let next = graph.node(&InputTypeConstraint::types(targets));
            graph.overload(&[candidate, *input, next], &alternatives, &self.operation);
            candidate = next;
        }
        let mut alternatives = Vec::new();
        let mut targets = Vec::new();
        let mut unordered = None;
        let candidate_types = graph.domain(candidate).clone();
        for source in candidate_types.admissible_types().unwrap_or(&[]) {
            let target = if *source == MinMaxExpr::unknown() {
                PgBuiltInType::text()
            } else {
                source.clone()
            };
            let ordering = if self.comparison {
                facts.ordered(&target)
            } else {
                Some(true)
            };
            if ordering.is_none() {
                graph
                    .common_problems
                    .push(crate::sem::problem::MinMaxProblem::missing_comparison_facts(&target));
            }
            if ordering == Some(false) {
                unordered.get_or_insert(target.clone());
            } else {
                alternatives.push(SignatureAlternative::new(&[
                    InputTypeConstraint::exact(source),
                    InputTypeConstraint::exact(&target),
                ]));
                targets.push(target);
            }
        }
        if targets.is_empty()
            && let Some(target) = unordered
        {
            graph
                .common_problems
                .push(crate::sem::problem::MinMaxProblem::not_ordered(&target));
        }
        graph.restrict(self.result, &InputTypeConstraint::types(targets));
        graph.overload(&[candidate, self.result], &alternatives, &self.operation);
        for input in &self.inputs {
            let mut alternatives = Vec::new();
            for target in graph.domain(self.result).admissible_types().unwrap_or(&[]) {
                let sources = graph
                    .domain(*input)
                    .admissible_types()
                    .unwrap_or(&[])
                    .iter()
                    .filter(|source| facts.coerce(source, target))
                    .cloned();
                alternatives.push(SignatureAlternative::new(&[
                    InputTypeConstraint::types(sources),
                    InputTypeConstraint::exact(target),
                ]));
            }
            graph.overload(&[*input, self.result], &alternatives, &self.operation);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    mod test_helpers;

    use crate::sem::parameter_constraints::{
        ConstraintGraph, InputTypeConstraint, ParameterConstraints, TypeProjection,
    };
    use crate::sem::{MinMaxExpr, PgBuiltInType};
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::promoted("int8", true)]
    #[case::wrong_result("int4", false)]
    #[case::wrong_category("text", false)]
    fn assignments_keep_input_result_correlations(#[case] result: &str, #[case] allowed: bool) {
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let output = graph.parameter(3);
        let result_node = graph.common_type(
            &test_helpers::context(),
            &[left, right],
            false,
            "common type",
        );
        graph.equal(result_node, output);
        graph.solve().unwrap();
        assert!(graph.domain(left).is_any());
        let constraints = ParameterConstraints::from_graph(&graph);
        assert_eq!(
            constraints.allows(&BTreeMap::from([
                (1, PgBuiltInType::int4()),
                (2, PgBuiltInType::int8()),
                (3, crate::sem::TypeReference::from_master_type_name(result))
            ])),
            allowed
        );
    }

    #[test]
    fn late_result_constraints_propagate_to_sources() {
        let mut graph = ConstraintGraph::default();
        let source = graph.parameter(1);
        let unknown = graph.node(&InputTypeConstraint::exact(&MinMaxExpr::unknown()));
        let result = graph.common_type(
            &test_helpers::context(),
            &[source, unknown],
            false,
            "common type",
        );
        graph.restrict(result, &InputTypeConstraint::exact(&PgBuiltInType::int4()));
        graph.solve().unwrap();
        assert_eq!(
            graph.domain(source),
            &InputTypeConstraint::exact(&PgBuiltInType::int4())
        );
    }

    #[rstest::rstest]
    fn array_construction_retains_scalar_and_array_sources(
        #[values(false, true)] array_source: bool,
    ) {
        let mut graph = ConstraintGraph::default();
        let source = graph.parameter(1);
        let result = graph.parameter(2);
        graph.project(source, result, TypeProjection::construct_array());
        graph.restrict(
            result,
            &InputTypeConstraint::exact(&PgBuiltInType::int4().array_type()),
        );
        graph.solve().unwrap();
        let source = if array_source {
            PgBuiltInType::int4().array_type()
        } else {
            PgBuiltInType::int4()
        };
        assert!(ParameterConstraints::from_graph(&graph).allows(&BTreeMap::from([(1, source)])));
    }
}
