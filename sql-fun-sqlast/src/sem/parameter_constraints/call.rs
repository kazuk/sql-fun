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
        signature: &Signature,
        inputs: &[InputTypeConstraint],
    ) -> Option<Self> {
        let mut sources = vec![HashMap::new(); inputs.len()];
        // Reject declarations using known operands before enumerating any
        // unrestricted operand's cast sources.
        for (index, input) in inputs.iter().enumerate() {
            if signature.accepts_any_type(index) {
                continue;
            }
            if let Some(types) = input.admissible_types() {
                sources[index] = Self::ranks(context, signature, index, types.iter().cloned());
                if sources[index].is_empty() {
                    return None;
                }
            }
        }
        for (index, input) in inputs.iter().enumerate() {
            if input.is_any() && !signature.accepts_any_type(index) {
                sources[index] = Self::ranks(
                    context,
                    signature,
                    index,
                    signature.source_types(context, index).into_iter(),
                );
            }
        }
        Some(Self {
            signature: signature.clone(),
            sources,
        })
    }

    fn ranks(
        context: &impl ParseContext,
        signature: &Signature,
        index: usize,
        sources: impl Iterator<Item = TypeReference>,
    ) -> HashMap<TypeReference, OverloadRank> {
        sources
            .filter_map(|source| {
                Some((
                    source.clone(),
                    signature.cast_score(context, &source, index)?,
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

/// Keep scalar declarations and array conversion evidence, rather than one
/// concrete polymorphic signature per catalog type. Facts outlive ParseContext
/// so explicit assignment inspection can instantiate a previously unseen array.
#[derive(Clone, Debug)]
struct CallFacts {
    postgres_defaults: bool,
    concrete: Vec<RankedSignature>,
    arrays: Vec<Signature>,
    array_casts: HashMap<TypeReference, HashMap<TypeReference, OverloadRank>>,
}

impl CallFacts {
    fn prepare(
        context: &impl ParseContext,
        signatures: &[Signature],
        inputs: &[InputTypeConstraint],
    ) -> Self {
        let concrete = signatures
            .iter()
            .filter(|signature| signature.is_concrete_for_arity(inputs.len()))
            .filter_map(|signature| RankedSignature::prepare(context, signature, inputs))
            .collect();
        let arrays_possible = Self::arrays_possible(context, inputs);
        let arrays: Vec<_> = signatures
            .iter()
            .filter(|signature| arrays_possible && signature.is_array_comparison())
            .cloned()
            .collect();
        let array_casts = if arrays.is_empty() {
            HashMap::new()
        } else {
            Self::array_conversion_ranks(context)
        };
        Self {
            postgres_defaults: context.dialect() == crate::ast::Dialect::postgres(),
            concrete,
            arrays,
            array_casts,
        }
    }

    fn arrays_possible(context: &impl ParseContext, inputs: &[InputTypeConstraint]) -> bool {
        inputs.iter().all(|input| {
            input.admissible_types().is_none_or(|types| {
                types.iter().any(|source| {
                    source.is_array() == Some(true)
                        || context
                            .get_implicit_castable(source)
                            .iter()
                            .any(|target| target.is_array() == Some(true))
                })
            })
        })
    }

    fn array_conversion_ranks(
        context: &impl ParseContext,
    ) -> HashMap<TypeReference, HashMap<TypeReference, OverloadRank>> {
        let mut ranks = HashMap::<_, HashMap<_, _>>::new();
        // Retain conversion edges, not comparison candidates. Unrelated
        // catalog types never become default source/target alternatives.
        for element in context.type_references() {
            if element.is_dynamic()
                || element.is_simple_infer_arg()
                || element.is_common_infer_arg()
            {
                continue;
            }
            let array = element.array_type();
            for target in context.get_implicit_castable(&element) {
                let target_array = target.array_type();
                if let Some(rank) =
                    OverloadRank::argument(context, Some(&array), Some(&target_array))
                {
                    ranks
                        .entry(array.clone())
                        .or_default()
                        .insert(target_array, rank);
                }
                // Includes vectors and declared scalar-to-array casts.
                if target.is_array() == Some(true)
                    && let Some(rank) =
                        OverloadRank::argument(context, Some(&element), Some(&target))
                {
                    ranks
                        .entry(element.clone())
                        .or_default()
                        .insert(target, rank);
                }
            }
        }
        ranks
    }

    const fn supplies_input_context(&self) -> bool {
        self.concrete.len() == 1 && self.arrays.is_empty()
    }

    const fn has_declarations(&self) -> bool {
        !self.concrete.is_empty() || !self.arrays.is_empty()
    }

    fn result_types(&self) -> InputTypeConstraint {
        InputTypeConstraint::types(
            self.concrete
                .iter()
                .map(|entry| entry.signature.result().clone())
                .chain(
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
            || self
                .concrete
                .iter()
                .all(|entry| result.allows(entry.signature.result()))
        {
            return None;
        }
        let viable: Vec<_> = self
            .concrete
            .iter()
            .filter(|entry| result.allows(entry.signature.result()))
            .collect();
        let first = viable.first()?;
        Some(
            (0..first.signature.inputs().len())
                .map(|index| {
                    if viable
                        .iter()
                        .any(|entry| entry.signature.accepts_any_type(index))
                    {
                        InputTypeConstraint::any()
                    } else {
                        InputTypeConstraint::types(
                            viable
                                .iter()
                                .flat_map(|entry| entry.sources[index].keys().cloned()),
                        )
                    }
                })
                .collect(),
        )
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

    fn signatures(&self, inputs: &[InputTypeConstraint]) -> Vec<RankedSignature> {
        let mut signatures = self.concrete.clone();
        let mut targets = HashSet::new();
        for source in inputs
            .iter()
            .filter_map(InputTypeConstraint::admissible_types)
            .flatten()
        {
            if source.is_array() == Some(true) {
                targets.insert(source.clone());
            }
            if let Some(casts) = self.array_casts.get(source) {
                targets.extend(casts.keys().cloned());
            }
        }
        for template in &self.arrays {
            for target in &targets {
                if self.concrete.iter().any(|signature| {
                    signature.signature.inputs() == [target.clone(), target.clone()]
                }) {
                    continue;
                }
                let mut sources: HashMap<_, _> = self
                    .array_casts
                    .iter()
                    .filter_map(|(source, casts)| Some((source.clone(), *casts.get(target)?)))
                    .collect();
                sources.entry(target.clone()).or_insert_with(|| {
                    OverloadRank::from(crate::sem::CastContext::no_conversion())
                });
                signatures.push(RankedSignature {
                    signature: Signature::new(&[target.clone(), target.clone()], template.result()),
                    sources: vec![sources.clone(), sources],
                });
            }
        }
        signatures.retain(|signature| signature.accepts_inputs(inputs));
        signatures
    }
}

#[derive(Clone, Debug)]
pub(super) struct CallConstraint {
    nodes: Vec<usize>,
    facts: Arc<CallFacts>,
    operation: String,
}

impl CallConstraint {
    pub(super) fn prepare(
        context: &impl ParseContext,
        nodes: &[usize],
        signatures: &[Signature],
        inputs: &[InputTypeConstraint],
        operation: &str,
    ) -> Self {
        Self {
            nodes: nodes.to_vec(),
            facts: Arc::new(CallFacts::prepare(context, signatures, inputs)),
            operation: operation.to_owned(),
        }
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
        // Wait for other occurrences, casts, projections and known operands.
        // A sole concrete declaration itself supplies SQL type context.
        let default_inference = self.facts.postgres_defaults
            && inputs.iter().all(InputTypeConstraint::is_any)
            && !self.facts.supplies_input_context();
        if default_inference {
            if !defaults {
                return false;
            }
            inputs.fill(InputTypeConstraint::types(default_types()));
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
mod tests;

fn combine(
    signatures: &[RankedSignature],
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
        alternatives.push(signatures[candidate].alternative(domains, excluded));
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
        signatures: &[RankedSignature],
        inputs: &[InputTypeConstraint],
    ) -> Vec<Vec<Self>> {
        let mut choices = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let mut sources = HashSet::new();
            if let Some(types) = input.admissible_types() {
                sources.extend(types.iter().cloned());
            } else {
                for signature in signatures {
                    sources.extend(signature.sources[index].keys().cloned());
                }
            }
            let has_unrestricted = signatures
                .iter()
                .any(|signature| signature.signature.accepts_any_type(index));
            let excluded: Vec<_> = sources.iter().cloned().collect();
            let mut groups = BTreeMap::<Vec<Option<OverloadRank>>, Vec<TypeReference>>::new();
            for source in sources {
                if !input.allows(&source) {
                    continue;
                }
                let scores = signatures
                    .iter()
                    .map(|signature| signature.score(&source, index))
                    .collect();
                groups.entry(scores).or_default().push(source);
            }
            let mut partitions: Vec<_> = groups
                .into_iter()
                .map(|(scores, types)| Self::new(&scores, &types))
                .collect();
            if input.is_any() && has_unrestricted {
                let scores: Vec<_> = signatures
                    .iter()
                    .map(|signature| {
                        if signature.signature.accepts_any_type(index) {
                            Some(OverloadRank::default())
                        } else {
                            None
                        }
                    })
                    .collect();
                partitions.push(Self {
                    scores: OverloadScores::new(&scores),
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
