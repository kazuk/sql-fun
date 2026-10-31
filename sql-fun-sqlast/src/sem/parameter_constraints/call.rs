//! Deferred overload relations. Retained ranks are inference facts, not a catalog.
use super::{ConstraintGraph, InputTypeConstraint, SignatureAlternative};
use crate::sem::overload_rank::OverloadRank;
use crate::sem::parameter_resolution::Signature;
use crate::sem::{ParseContext, TypeReference};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

pub(super) fn default_types() -> [TypeReference; 7] {
    [
        "bool", "int4", "int8", "float4", "float8", "text", "numeric",
    ]
    .map(TypeReference::from_master_type_name)
}

#[derive(Clone, Debug)]
struct RankedSignature {
    signature: Signature,
    sources: Vec<HashMap<TypeReference, OverloadRank>>,
}

impl RankedSignature {
    fn prepare(
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        signature: &Signature,
        candidates: &HashSet<TypeReference>,
    ) -> Self {
        let sources = (0..signature.inputs().len())
            .map(|index| {
                if signature.accepts_any_type(index) {
                    HashMap::new()
                } else {
                    Self::ranks(context, cache, signature, index, candidates.iter().cloned())
                }
            })
            .collect();
        Self {
            signature: signature.clone(),
            sources,
        }
    }

    fn ranks(
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        signature: &Signature,
        index: usize,
        sources: impl Iterator<Item = TypeReference>,
    ) -> HashMap<TypeReference, OverloadRank> {
        sources
            .filter_map(|source| {
                Some((
                    source.clone(),
                    cache.argument_rank(context, signature, index, &source)?,
                ))
            })
            .collect()
    }

    fn accepts_inputs(&self, inputs: &[InputTypeConstraint]) -> bool {
        inputs.iter().enumerate().all(|(index, input)| {
            input.admissible_types().is_none_or(|types| {
                types
                    .iter()
                    .any(|source| self.score(source, index).is_some())
            })
        })
    }

    fn score(&self, source: &TypeReference, index: usize) -> Option<OverloadRank> {
        if self.signature.accepts_any_type(index) {
            Some(OverloadRank::default())
        } else {
            self.sources[index].get(source).copied()
        }
    }

    fn unrestricted_score(&self, index: usize) -> Option<OverloadRank> {
        if self.signature.accepts_any_type(index) {
            Some(OverloadRank::default())
        } else {
            None
        }
    }

    fn accepts_default_inputs(&self) -> bool {
        (0..self.signature.inputs().len()).all(|index| {
            default_types()
                .iter()
                .any(|ty| self.score(ty, index).is_some())
        })
    }

    fn retain_sources(
        &mut self,
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        sources: &HashSet<TypeReference>,
    ) {
        for index in 0..self.sources.len() {
            if !self.signature.accepts_any_type(index) {
                self.sources[index].extend(Self::ranks(
                    context,
                    cache,
                    &self.signature,
                    index,
                    sources.iter().cloned(),
                ));
            }
        }
    }

    fn alternative(
        &self,
        inputs: &[InputTypeConstraint],
        excluded: &[Vec<TypeReference>],
    ) -> SignatureAlternative {
        let mut domains = inputs.to_vec();
        domains.push(InputTypeConstraint::exact(self.signature.result()));
        SignatureAlternative::excluding(&domains, excluded)
    }
}

/// Cast ranks indexed by source; all storage queries stay with this collection.
#[derive(Clone, Debug, Default)]
struct ArrayCastRanks(HashMap<TypeReference, HashMap<TypeReference, OverloadRank>>);

impl ArrayCastRanks {
    fn sources_for_target(&self, target: &TypeReference) -> HashMap<TypeReference, OverloadRank> {
        self.0
            .iter()
            .filter_map(|(source, casts)| Some((source.clone(), *casts.get(target)?)))
            .collect()
    }

    fn targets_for_source(&self, source: &TypeReference) -> HashSet<TypeReference> {
        let mut targets = HashSet::new();
        if source.is_array() == Some(true) {
            targets.insert(source.clone());
        }
        if let Some(casts) = self.0.get(source) {
            targets.extend(casts.keys().cloned());
        }
        targets
    }

    fn retain_source(&mut self, context: &impl ParseContext, source: &TypeReference) {
        if self.0.contains_key(source) {
            return;
        }
        let mut targets = context.get_implicit_castable(source);
        if source.is_array() == Some(true) {
            targets.insert(source.clone());
            targets.extend(
                context
                    .get_implicit_castable(&source.element_type())
                    .into_iter()
                    .map(|ty| ty.array_type()),
            );
        }
        let ranks = targets
            .into_iter()
            .filter(|target| target.is_array() == Some(true))
            .filter_map(|target| {
                Some((
                    target.clone(),
                    OverloadRank::argument(context, Some(source), Some(&target))?
                        .instantiated_operator(),
                ))
            })
            .collect();
        self.0.insert(source.clone(), ranks);
    }
}

/// Ordered overload candidates and their input-domain/ranking queries.
#[derive(Clone, Debug)]
struct RankedSignatures(Vec<RankedSignature>);

impl RankedSignatures {
    const fn len(&self) -> usize {
        self.0.len()
    }

    const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn sources_for_argument(&self, index: usize) -> HashSet<TypeReference> {
        self.0
            .iter()
            .flat_map(|entry| entry.sources[index].keys().cloned())
            .collect()
    }

    fn accepts_unrestricted_argument(&self, index: usize) -> bool {
        self.0
            .iter()
            .any(|entry| entry.signature.accepts_any_type(index))
    }

    fn scores_for_source(&self, source: &TypeReference, index: usize) -> Vec<Option<OverloadRank>> {
        self.0
            .iter()
            .map(|entry| entry.score(source, index))
            .collect()
    }

    fn unrestricted_scores(&self, index: usize) -> OverloadScores {
        let scores: Vec<_> = self
            .0
            .iter()
            .map(|entry| entry.unrestricted_score(index))
            .collect();
        OverloadScores::new(&scores)
    }

    fn input_domain(&self, index: usize) -> InputTypeConstraint {
        if self.accepts_unrestricted_argument(index) {
            InputTypeConstraint::any()
        } else {
            InputTypeConstraint::types(self.sources_for_argument(index))
        }
    }

    fn input_domains(&self) -> Vec<InputTypeConstraint> {
        let Some(first) = self.0.first() else {
            return Vec::new();
        };
        (0..first.signature.inputs().len())
            .map(|index| self.input_domain(index))
            .collect()
    }

    fn accepts_default_inputs(&self) -> bool {
        self.0.iter().any(RankedSignature::accepts_default_inputs)
    }

    fn result_types(&self) -> HashSet<TypeReference> {
        self.0
            .iter()
            .map(|entry| entry.signature.result().clone())
            .collect()
    }

    fn all_results_allowed(&self, result: &InputTypeConstraint) -> bool {
        self.0
            .iter()
            .all(|entry| result.allows(entry.signature.result()))
    }

    fn permitting_result(&self, result: &InputTypeConstraint) -> Self {
        Self(
            self.0
                .iter()
                .filter(|entry| result.allows(entry.signature.result()))
                .cloned()
                .collect(),
        )
    }

    fn retain_sources(
        &mut self,
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        sources: &HashSet<TypeReference>,
    ) {
        for entry in &mut self.0 {
            entry.retain_sources(context, cache, sources);
        }
    }

    fn has_binary_inputs(&self, target: &TypeReference) -> bool {
        self.0
            .iter()
            .any(|entry| entry.signature.inputs() == [target.clone(), target.clone()])
    }

    fn push_array_comparison(
        &mut self,
        target: &TypeReference,
        result: &TypeReference,
        mut sources: HashMap<TypeReference, OverloadRank>,
    ) {
        sources.entry(target.clone()).or_insert_with(|| {
            OverloadRank::from(crate::sem::CastContext::no_conversion()).instantiated_operator()
        });
        self.0.push(RankedSignature {
            signature: Signature::new(&[target.clone(), target.clone()], result),
            sources: vec![sources.clone(), sources],
        });
    }

    fn retain_inputs(&mut self, inputs: &[InputTypeConstraint]) {
        self.0.retain(|entry| entry.accepts_inputs(inputs));
    }

    fn alternative(
        &self,
        candidate: usize,
        inputs: &[InputTypeConstraint],
        excluded: &[Vec<TypeReference>],
    ) -> SignatureAlternative {
        self.0[candidate].alternative(inputs, excluded)
    }
}

/// Owned ranks for finite declarations, defaults, and encountered SQL types.
/// No catalog type universe is captured or expanded.
#[derive(Clone, Debug)]
struct CallFacts {
    postgres_defaults: bool,
    concrete: RankedSignatures,
    arrays: Vec<Signature>,
    array_casts: ArrayCastRanks,
}

impl CallFacts {
    fn prepare(
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        signatures: &[Signature],
        inputs: &[InputTypeConstraint],
    ) -> Self {
        let declarations: Vec<_> = signatures
            .iter()
            .filter(|signature| signature.is_concrete_for_arity(inputs.len()))
            .collect();
        // Declared cast sources are finite inspection facts. They never
        // enlarge the seven candidates used by default placeholder inference.
        let targets: HashSet<_> = declarations
            .iter()
            .flat_map(|signature| signature.inputs().iter().cloned())
            .collect();
        let candidates: HashSet<_> = default_types()
            .into_iter()
            .chain(
                targets
                    .iter()
                    .flat_map(|target| context.declared_implicit_cast_sources(target)),
            )
            .collect();
        let concrete = declarations
            .into_iter()
            .map(|signature| RankedSignature::prepare(context, cache, signature, &candidates))
            .collect();
        let arrays = signatures
            .iter()
            .filter(|signature| signature.is_array_comparison())
            .cloned()
            .collect();
        let mut facts = Self {
            postgres_defaults: context.dialect() == crate::ast::Dialect::postgres(),
            concrete: RankedSignatures(concrete),
            arrays,
            array_casts: ArrayCastRanks::default(),
        };
        // Preserve actual array casts for the finite declared/default types so
        // explicit assignment inspection can outlive the borrowed context.
        for source in &candidates {
            facts.retain_array_source(context, &source.array_type());
        }
        facts.retain(
            context,
            cache,
            inputs
                .iter()
                .filter_map(InputTypeConstraint::admissible_types)
                .flatten(),
        );
        facts
    }

    fn retain_array_source(&mut self, context: &impl ParseContext, source: &TypeReference) {
        if !self.arrays.is_empty() {
            self.array_casts.retain_source(context, source);
        }
    }

    fn retain<'a>(
        &mut self,
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        types: impl Iterator<Item = &'a TypeReference>,
    ) {
        let mut sources = HashSet::new();
        for ty in types {
            sources.insert(ty.clone());
            // A specific SQL-established type justifies its incoming casts.
            // Never enumerate all catalog types to find an initial seed.
            sources.extend(context.declared_implicit_cast_sources(ty));
            self.retain_array_source(context, ty);
        }
        self.concrete.retain_sources(context, cache, &sources);
        for source in sources {
            self.retain_array_source(context, &source);
        }
    }

    fn supplies_input_context(&self) -> bool {
        self.arrays.is_empty()
            && (self.concrete.len() == 1 || !self.concrete.accepts_default_inputs())
    }

    fn declared_inputs(&self) -> Vec<InputTypeConstraint> {
        self.concrete.input_domains()
    }

    const fn has_declarations(&self) -> bool {
        !self.concrete.is_empty() || !self.arrays.is_empty()
    }

    fn result_types(&self) -> InputTypeConstraint {
        InputTypeConstraint::types(
            self.concrete.result_types().into_iter().chain(
                self.arrays
                    .iter()
                    .map(|signature| signature.result().clone()),
            ),
        )
    }

    fn contextual_inputs(&self, result: &InputTypeConstraint) -> Option<Vec<InputTypeConstraint>> {
        if result.is_any()
            || self
                .arrays
                .iter()
                .any(|signature| result.allows(signature.result()))
            || self.concrete.all_results_allowed(result)
        {
            return None;
        }
        let viable = self.concrete.permitting_result(result);
        if viable.is_empty() {
            return None;
        }
        Some(viable.input_domains())
    }

    fn alternatives(
        &self,
        inputs: &[InputTypeConstraint],
        result: &InputTypeConstraint,
    ) -> Vec<SignatureAlternative> {
        let signatures = self.signatures(inputs);
        let choices = SourcePartition::argument_choices(&signatures, inputs);
        let mut alternatives = Vec::new();
        combine(
            &signatures,
            &choices,
            0,
            &[],
            &[],
            &OverloadScores::initial(signatures.len()),
            &mut alternatives,
        );
        // Filter results after ranking so return-type context cannot promote
        // an overload that lost for the same concrete input assignment.
        alternatives.retain(|alternative| alternative.permits_result(result));
        alternatives
    }

    fn signatures(&self, inputs: &[InputTypeConstraint]) -> RankedSignatures {
        let mut signatures = self.concrete.clone();
        let mut targets = HashSet::new();
        for source in inputs
            .iter()
            .filter_map(InputTypeConstraint::admissible_types)
            .flatten()
        {
            targets.extend(self.array_casts.targets_for_source(source));
        }
        for template in &self.arrays {
            for target in &targets {
                if self.concrete.has_binary_inputs(target) {
                    continue;
                }
                signatures.push_array_comparison(
                    target,
                    template.result(),
                    self.array_casts.sources_for_target(target),
                );
            }
        }
        signatures.retain_inputs(inputs);
        signatures
    }
}

#[cfg(test)]
mod call_facts_tests {
    use super::{CallFacts, InputTypeConstraint, default_types};
    use crate::test_helpers::parameter_calls as test_helpers;

    #[test]
    fn unrelated_catalog_growth_does_not_instantiate_array_comparisons() {
        let (mut context, signatures) = test_helpers::comparison();
        context.forbid_type_enumeration();
        for index in 0..50 {
            context.setup_type(format!("public.unrelated_{index}").parse().unwrap());
        }
        let facts = CallFacts::prepare(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &signatures,
            &[InputTypeConstraint::any(), InputTypeConstraint::any()],
        );
        let defaults = InputTypeConstraint::types(default_types());
        let instantiated = facts.signatures(&[defaults.clone(), defaults]);
        assert_eq!(instantiated.len(), 7);
        assert_eq!(
            instantiated.input_domains(),
            vec![InputTypeConstraint::types(default_types()); 2]
        );
    }
}

#[derive(Clone, Debug)]
pub(super) struct CallConstraint {
    nodes: Vec<usize>,
    facts: Arc<CallFacts>,
    operation: String,
    parameters: Vec<bool>,
}

impl CallConstraint {
    pub(super) fn prepare(
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        nodes: &[usize],
        signatures: &[Signature],
        inputs: &[InputTypeConstraint],
        operation: &str,
        parameters: &[bool],
    ) -> Self {
        Self {
            parameters: parameters.to_vec(),
            nodes: nodes.to_vec(),
            facts: Arc::new(CallFacts::prepare(context, cache, signatures, inputs)),
            operation: operation.to_owned(),
        }
    }

    pub(super) fn retain<'a>(
        &mut self,
        context: &impl ParseContext,
        cache: &mut crate::CacheContext,
        types: impl Iterator<Item = &'a TypeReference>,
    ) {
        Arc::make_mut(&mut self.facts).retain(context, cache, types);
    }

    pub(super) fn instantiate(&self, graph: &mut ConstraintGraph, defaults: bool) -> bool {
        let mut inputs: Vec<_> = self.nodes[..self.nodes.len() - 1]
            .iter()
            .map(|node| graph.domain(*node).clone())
            .collect();
        let result = graph
            .domain(*self.nodes.last().expect("call result"))
            .clone();
        if inputs.iter().all(InputTypeConstraint::is_any)
            && let Some(contextual) = self.facts.contextual_inputs(&result)
        {
            inputs = contextual;
        }
        if inputs.iter().all(InputTypeConstraint::is_any) && self.facts.supplies_input_context() {
            inputs = self.facts.declared_inputs();
        }
        // Wait for other occurrences, casts, projections and known operands.
        // A sole concrete declaration itself supplies SQL type context.
        let default_inference = self.facts.postgres_defaults
            && inputs.iter().all(InputTypeConstraint::is_any)
            && !self.facts.supplies_input_context();
        if default_inference {
            if !defaults {
                return false;
            }
            for (input, parameter) in inputs.iter_mut().zip(&self.parameters) {
                if *parameter {
                    *input = InputTypeConstraint::types(default_types());
                }
            }
        }
        if defaults
            && inputs.iter().all(InputTypeConstraint::is_any)
            && self
                .facts
                .arrays
                .iter()
                .any(|signature| result.allows(signature.result()))
        {
            graph.unsupported_inference.push(self.operation.clone());
            graph.restrict(
                *self.nodes.last().expect("call result"),
                &self.facts.result_types(),
            );
            return true;
        }
        let alternatives = self.facts.alternatives(&inputs, &result);
        if default_inference && alternatives.is_empty() && self.facts.has_declarations() {
            // Missing default witnesses do not establish invalid SQL. Keep the
            // relationship for explicit assignment inspection and report the
            // still-unresolved application inputs at the normal boundary.
            graph.restrict(
                *self.nodes.last().expect("call result"),
                &self.facts.result_types(),
            );
            return false;
        }
        graph.overload(&self.nodes, &alternatives, &self.operation);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstraintGraph, InputTypeConstraint};
    use crate::sem::ParameterConstraints;
    use crate::sem::{PgBuiltInType, TypeReference};
    use crate::test_helpers::parameter_calls as test_helpers;
    use std::collections::BTreeMap;

    #[test]
    fn defaults_are_bounded_without_restricting_explicit_assignments() {
        let (mut context, signatures) = test_helpers::comparison();
        context.forbid_type_enumeration();
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let result = graph.node(&InputTypeConstraint::any());
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "comparison",
        );
        graph.solve().unwrap();
        assert_eq!(
            graph.domain(left),
            &InputTypeConstraint::types(
                [
                    "bool", "int4", "int8", "float4", "float8", "text", "numeric"
                ]
                .map(TypeReference::from_master_type_name)
            )
        );
        assert_eq!(graph.domain(right), graph.domain(left));
        let constraints = ParameterConstraints::from_graph(&graph);
        drop(context);
        let bytea = TypeReference::from_master_type_name("bytea");
        assert!(constraints.allows(&BTreeMap::from([(1, bytea.clone()), (2, bytea)])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::bool()),
            (2, PgBuiltInType::int4())
        ])));
        let custom_array: TypeReference = "public.custom[]".parse().unwrap();
        assert!(constraints.allows(&BTreeMap::from([
            (1, custom_array.clone()),
            (2, custom_array)
        ])));
    }

    #[rstest::rstest]
    fn later_array_information_survives_earlier_default_inference(
        #[values(false, true)] intermediate_solve: bool,
    ) {
        let (context, signatures) = test_helpers::comparison();
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let result = graph.node(&InputTypeConstraint::any());
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "comparison",
        );
        if intermediate_solve {
            graph.solve().unwrap();
        }
        let array = PgBuiltInType::int4().array_type();
        graph.restrict(left, &InputTypeConstraint::exact(&array));
        graph.solve().unwrap();
        assert_eq!(graph.domain(right).single_type(), Some(array.clone()));
        let constraints = ParameterConstraints::from_graph(&graph);
        assert!(constraints.allows(&BTreeMap::from([(2, array)])));
        assert!(!constraints.allows(&BTreeMap::from([(2, PgBuiltInType::text())])));
    }

    #[rstest::rstest]
    fn array_only_unknown_inputs_remain_unresolved_and_inspectable(
        #[values(false, true)] scalar_alternative: bool,
    ) {
        let (context, signatures) = test_helpers::comparison();
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let result = graph.node(&InputTypeConstraint::any());
        graph.restrict(result, &InputTypeConstraint::exact(&PgBuiltInType::bool()));
        let mut array_signatures = signatures[8..].to_vec();
        if scalar_alternative {
            array_signatures.push(crate::sem::parameter_resolution::Signature::new(
                &[PgBuiltInType::int4(), PgBuiltInType::int4()],
                &PgBuiltInType::int4(),
            ));
        }
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &array_signatures,
            "array comparison",
        );
        graph.solve().unwrap();
        assert!(graph.domain(left).is_any());
        assert_eq!(
            graph.domain(result).single_type(),
            Some(PgBuiltInType::bool())
        );
        let constraints = ParameterConstraints::from_graph(&graph);
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4().array_type())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    }

    #[test]
    fn concrete_array_declaration_precedes_polymorphic_instantiation() {
        let (context, mut signatures) = test_helpers::comparison();
        let array = PgBuiltInType::int4().array_type();
        signatures.push(crate::sem::parameter_resolution::Signature::new(
            &[array.clone(), array.clone()],
            &PgBuiltInType::int4(),
        ));
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        graph.restrict(left, &InputTypeConstraint::exact(&array));
        graph.restrict(right, &InputTypeConstraint::exact(&array));
        let result = graph.node(&InputTypeConstraint::any());
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "custom comparison",
        );
        graph.solve().unwrap();
        assert_eq!(
            graph.domain(result).single_type(),
            Some(PgBuiltInType::int4())
        );
    }

    #[rstest::rstest]
    fn explicit_array_assignments_use_retained_array_casts(
        #[values(false, true)] array_cast: bool,
    ) {
        let (context, signatures) = test_helpers::numeric_array_comparison(array_cast);
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let result = graph.node(&InputTypeConstraint::any());
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "comparison",
        );
        graph.solve().unwrap();
        let constraints = ParameterConstraints::from_graph(&graph);
        drop(context);
        assert_eq!(
            constraints.allows(&BTreeMap::from([
                (1, PgBuiltInType::int4().array_type()),
                (2, PgBuiltInType::int8().array_type()),
            ])),
            array_cast
        );
    }

    #[test]
    fn result_context_keeps_non_default_input_types_before_defaulting() {
        let (mut context, _) = test_helpers::comparison();
        let date = TypeReference::from_master_type_name("date");
        context.setup_type(date.clone());
        let signatures = [
            crate::sem::parameter_resolution::Signature::new(std::slice::from_ref(&date), &date),
            crate::sem::parameter_resolution::Signature::new(
                &[PgBuiltInType::int4()],
                &PgBuiltInType::int4(),
            ),
        ];
        let mut graph = ConstraintGraph::default();
        let input = graph.parameter(1);
        let result = graph.node(&InputTypeConstraint::exact(&date));
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[input, result],
            &signatures,
            "result-constrained call",
        );
        graph.solve().unwrap();
        assert_eq!(graph.domain(input).single_type(), Some(date.clone()));
        let constraints = ParameterConstraints::from_graph(&graph);
        assert!(constraints.allows(&BTreeMap::from([(1, date)])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    }

    #[test]
    fn unknown_expression_results_report_unsupported_inference_without_invalid_sql() {
        let (mut context, signatures) = test_helpers::comparison();
        context.forbid_type_enumeration();
        let mut graph = ConstraintGraph::default();
        let left = graph.node(&InputTypeConstraint::any());
        let right = graph.node(&InputTypeConstraint::any());
        let result = graph.node(&InputTypeConstraint::any());
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "comparison",
        );
        graph.solve().unwrap();
        graph.report_inference_problems(&mut context).unwrap();
        assert_eq!(context.reported_problems().len(), 1);
        assert!(context.has_reported_problem("inference_candidate_space_unsupported"));
        assert!(graph.domain(left).is_any());
        assert_eq!(
            graph.domain(result).single_type(),
            Some(PgBuiltInType::bool())
        );
    }

    #[test]
    fn sql_context_resolves_expression_results_before_the_unsupported_boundary() {
        let (mut context, signatures) = test_helpers::comparison();
        context.forbid_type_enumeration();
        let mut graph = ConstraintGraph::default();
        let left = graph.node(&InputTypeConstraint::any());
        let right = graph.node(&InputTypeConstraint::any());
        let result = graph.node(&InputTypeConstraint::any());
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "comparison",
        );
        let custom: TypeReference = "public.custom[]".parse().unwrap();
        graph.restrict(left, &InputTypeConstraint::exact(&custom));
        graph.prepare_projections(&context, &mut crate::test_helpers::create_cache_context());
        graph.solve().unwrap();
        graph.report_inference_problems(&mut context).unwrap();
        assert!(context.reported_problems().is_empty());
        assert_eq!(graph.domain(right).single_type(), Some(custom));
    }

    #[rstest::rstest]
    #[case::domain_exact(true, "text")]
    #[case::base_best_match(false, "bool")]
    fn domain_operator_ranking_survives_deferred_correlated_constraints(
        #[case] domain_right: bool,
        #[case] result_type: &str,
        #[values(false, true)] intermediate_solve: bool,
    ) {
        let mut context = crate::test_helpers::domain_casts::context();
        let domain = crate::test_helpers::domain_casts::domain(
            &mut context,
            "amount",
            Some(&PgBuiltInType::int4()),
        );
        let overloads = crate::test_helpers::domain_casts::operators(&domain);
        let signatures = crate::sem::OperatorDefinition::binary(&overloads).parameter_signatures();
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let result = graph.node(&InputTypeConstraint::any());
        graph.restrict(left, &InputTypeConstraint::exact(&domain));
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "domain comparison",
        );
        if intermediate_solve {
            graph.solve().unwrap();
        }
        let right_type = if domain_right {
            domain.clone()
        } else {
            PgBuiltInType::int4()
        };
        graph.restrict(right, &InputTypeConstraint::exact(&right_type));
        graph.solve().unwrap();
        assert_eq!(
            graph.domain(result).single_type(),
            Some(TypeReference::from_master_type_name(result_type))
        );
        let constraints = ParameterConstraints::from_graph(&graph);
        drop(context);
        assert!(constraints.allows(&BTreeMap::from([(1, domain), (2, right_type)])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
    }

    #[test]
    fn exact_domain_over_array_precedes_an_instantiated_array_operator() {
        let mut context = crate::test_helpers::domain_casts::context();
        let domain = crate::test_helpers::domain_casts::domain(
            &mut context,
            "amounts",
            Some(&PgBuiltInType::int4().array_type()),
        );
        let anyarray = TypeReference::from_master_type_name("anyarray");
        let signatures = [
            crate::sem::parameter_resolution::Signature::new(
                &[domain.clone(), domain.clone()],
                &PgBuiltInType::text(),
            ),
            crate::sem::parameter_resolution::Signature::new(
                &[anyarray.clone(), anyarray],
                &PgBuiltInType::bool(),
            ),
        ];
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        graph.restrict(left, &InputTypeConstraint::exact(&domain));
        graph.restrict(right, &InputTypeConstraint::exact(&domain));
        let result = graph.node(&InputTypeConstraint::any());
        graph.call(
            &context,
            &mut crate::test_helpers::create_cache_context(),
            &[left, right, result],
            &signatures,
            "domain array comparison",
        );
        graph.solve().unwrap();
        assert_eq!(
            graph.domain(result).single_type(),
            Some(PgBuiltInType::text())
        );
    }
}

fn combine(
    signatures: &RankedSignatures,
    choices: &[Vec<SourcePartition>],
    index: usize,
    domains: &[InputTypeConstraint],
    excluded: &[Vec<TypeReference>],
    scores: &OverloadScores,
    alternatives: &mut Vec<SignatureAlternative>,
) {
    if index == choices.len() {
        let Some(candidate) = scores.unique_best() else {
            return;
        };
        alternatives.push(signatures.alternative(candidate, domains, excluded));
        return;
    }
    for choice in &choices[index] {
        let next_scores = choice.accumulate_scores(scores);
        if next_scores.has_no_candidates() {
            continue;
        }
        let domains = choice.extend_domains(domains);
        let mut excluded = excluded.to_vec();
        excluded.push(choice.excluded.clone());
        crate::sem::with_stack(|| {
            combine(
                signatures,
                choices,
                index + 1,
                &domains,
                &excluded,
                &next_scores,
                alternatives,
            )
        });
    }
}

struct SourcePartition {
    scores: OverloadScores,
    domain: InputTypeConstraint,
    excluded: Vec<TypeReference>,
}

impl SourcePartition {
    // Sources in one partition have identical cast scores for every overload.
    fn argument_choices(
        signatures: &RankedSignatures,
        inputs: &[InputTypeConstraint],
    ) -> Vec<Vec<Self>> {
        let mut choices = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let sources = if let Some(types) = input.admissible_types() {
                types.iter().cloned().collect()
            } else {
                signatures.sources_for_argument(index)
            };
            let excluded: Vec<_> = sources.iter().cloned().collect();
            let mut groups = BTreeMap::<Vec<Option<OverloadRank>>, Vec<TypeReference>>::new();
            for source in sources {
                if !input.allows(&source) {
                    continue;
                }
                let scores = signatures.scores_for_source(&source, index);
                groups.entry(scores).or_default().push(source);
            }
            let mut partitions: Vec<_> = groups
                .into_iter()
                .map(|(scores, types)| Self::new(&scores, &types))
                .collect();
            if input.is_any() && signatures.accepts_unrestricted_argument(index) {
                partitions.push(Self {
                    scores: signatures.unrestricted_scores(index),
                    domain: InputTypeConstraint::any(),
                    excluded,
                });
            }
            choices.push(partitions);
        }
        choices
    }

    fn new(scores: &[Option<OverloadRank>], types: &[TypeReference]) -> Self {
        Self {
            scores: OverloadScores::new(scores),
            domain: InputTypeConstraint::types(types.to_vec()),
            excluded: Vec::new(),
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
