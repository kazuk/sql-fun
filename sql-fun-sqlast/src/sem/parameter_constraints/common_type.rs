//! Owned common-type facts and relationships for initially unrestricted inputs.
//! These are inference data, not a second catalog or analysis environment.
use crate::context::{CastInfoRead, SchemaFileContext};
use crate::sem::parameter_constraints::{
    ConstraintGraph, InputTypeConstraint, SignatureAlternative,
};
use crate::sem::{MinMaxExpr, ParseContext, PgBuiltInType, TypeReference};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Debug)]
struct TypeFacts {
    base: TypeReference,
    category: Option<(char, bool)>,
    targets: HashSet<TypeReference>,
    ordered: Option<bool>,
}

/// Original catalog and explicit configuration for post-analysis inspection.
/// The frozen base is shared; only the current catalog layer is copied.
#[derive(Debug)]
struct CommonTypeEnvironment {
    catalog: crate::context::FrozenBaseContext,
    args: sql_fun_core::SqlFunArgs,
    metadata: sql_fun_core::SqlFunMetadata,
}

impl CommonTypeEnvironment {
    fn new(context: &impl ParseContext) -> Self {
        Self {
            catalog: context.retain_catalog(),
            args: context.get_args().clone(),
            metadata: context.metadata().clone(),
        }
    }

    fn context(&self) -> SchemaFileContext {
        SchemaFileContext::new(
            std::path::Path::new("<assignment>"),
            &self.catalog,
            &self.args,
            &self.metadata,
        )
    }
}

/// Query-owned semantic facts, prepared only for encountered types.
#[derive(Clone, Debug)]
pub(super) struct CommonTypeFacts {
    types: HashMap<TypeReference, TypeFacts>,
    environment: Arc<CommonTypeEnvironment>,
}

impl CommonTypeFacts {
    pub(super) fn prepare(context: &impl ParseContext) -> Self {
        Self {
            types: HashMap::new(),
            environment: Arc::new(CommonTypeEnvironment::new(context)),
        }
    }

    fn permits_defaults(&self) -> bool {
        crate::ast::Dialect::from(&self.environment.metadata) == crate::ast::Dialect::postgres()
    }

    pub(super) fn retain(&mut self, context: &impl ParseContext, ty: &TypeReference) {
        if self.types.contains_key(ty) {
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
        let base = facts.base.clone();
        self.types.insert(ty.clone(), facts.clone());
        self.types.insert(canonical, facts);
        if base != *ty {
            self.retain(context, &base);
        }
        if ty.is_array() == Some(true) {
            self.retain(context, &ty.element_type());
        }
        if let Some(definition) = ty.type_definition()
            && let Some(base) = definition.domain_base()
        {
            self.retain(context, base);
        }
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
        match self.types.get(&Self::identity(ty)) {
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
        if let Some(facts) = self.types.get(ty)
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
        if let Some(facts) = self.types.get(&base)
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
        // Mirror context-backed coercion: target domains accept conversions
        // through their bases while selection retains nominal array identity.
        let source = self.base(source);
        let target = self.base(target);
        if Self::identity(&source) == Self::identity(&target) {
            return true;
        }
        if source.is_array() == Some(true) && target.is_array() == Some(true) {
            return self.coerce(&source.element_type(), &target.element_type());
        }
        if self
            .types
            .get(&source)
            .is_some_and(|facts| facts.targets.contains(&Self::identity(&target)))
        {
            return true;
        }
        source.implicit_match_owned(&target)
    }

    fn select(&mut self, left: &TypeReference, right: &TypeReference) -> Option<TypeReference> {
        if left == right {
            return Some(left.clone());
        }
        if !self.types.contains_key(left) || !self.types.contains_key(right) {
            let context = self.environment.context();
            self.retain(&context, left);
            self.retain(&context, right);
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

    /// Retain the same semantic calculations for newly supplied assignments.
    fn choices(&mut self, seeds: Vec<TypeReference>, expand_sources: bool) -> InputTypeConstraint {
        let context = self.environment.context();
        let mut types: HashSet<_> = seeds
            .into_iter()
            .filter(|ty| *ty != MinMaxExpr::unknown())
            .collect();
        if expand_sources {
            // A concrete SQL/assignment target justifies its incoming conversions.
            for target in types.clone() {
                types.extend(context.declared_implicit_cast_sources(&target));
            }
        }
        for ty in types.clone() {
            self.retain(&context, &ty);
            let base = Self::catalog_base_type(&context, &ty);
            self.retain(&context, &base);
            if ty.is_array() == Some(true) {
                self.retain(&context, &ty.element_type());
            }
        }
        InputTypeConstraint::types(types)
    }
}

#[cfg(test)]
mod test_common_type_facts {
    use crate::sem::{MinMaxExpr, TypeReference};
    use crate::test_helpers::common_types as test_helpers;

    #[rstest::rstest]
    #[case::domain_first("public.amount[]", "pg_catalog.int4[]", "public.amount[]")]
    #[case::base_first("pg_catalog.int4[]", "public.amount[]", "pg_catalog.int4[]")]
    #[case::nested_elements(
        "public.nested_amount[]",
        "pg_catalog.int4[]",
        "public.nested_amount[]"
    )]
    #[case::promoted("public.amount[]", "pg_catalog.int8[]", "pg_catalog.int8[]")]
    #[case::array_domain("public.amount_array", "pg_catalog.int4[]", "pg_catalog.int4[]")]
    #[case::nested_array_domain(
        "public.nested_amount_array",
        "pg_catalog.int4[]",
        "pg_catalog.int4[]"
    )]
    fn domain_array_coercions_keep_candidate_identity(
        #[case] left: &str,
        #[case] right: &str,
        #[case] result: &str,
    ) {
        let left: TypeReference = left.parse().unwrap();
        let right: TypeReference = right.parse().unwrap();
        let result: TypeReference = result.parse().unwrap();
        let context = test_helpers::domain_context();
        assert_eq!(
            MinMaxExpr::select_common_type(&context, &[left.clone(), right.clone()]).unwrap(),
            result
        );
        assert!(MinMaxExpr::can_coerce(&context, &left, &result));
        assert!(MinMaxExpr::can_coerce(&context, &right, &result));

        let mut facts = super::CommonTypeFacts::prepare(&context);
        drop(context);
        assert_eq!(facts.select(&left, &right), Some(result.clone()));
        assert!(facts.coerce(&left, &result));
        assert!(facts.coerce(&right, &result));
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

    pub(super) fn instantiate(
        &self,
        graph: &mut ConstraintGraph,
        facts: &mut CommonTypeFacts,
        defaults: bool,
    ) -> bool {
        let Some(choices) = self.candidates(graph, facts, defaults) else {
            return false;
        };
        for input in &self.inputs {
            if graph.domain(*input).is_any() {
                graph.restrict(*input, &choices);
            }
        }
        let candidate = self.select_inputs(graph, facts);
        self.constrain_result(graph, facts, candidate);
        self.constrain_coercions(graph, facts);
        true
    }

    fn candidates(
        &self,
        graph: &ConstraintGraph,
        facts: &mut CommonTypeFacts,
        defaults: bool,
    ) -> Option<InputTypeConstraint> {
        let mut seeds = self
            .inputs
            .iter()
            .chain(std::iter::once(&self.result))
            .filter_map(|node| graph.domain(*node).admissible_types())
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let unseeded = seeds.iter().all(|ty| *ty == MinMaxExpr::unknown());
        if unseeded {
            if !defaults
                || !facts.permits_defaults()
                || !self.inputs.iter().any(|node| graph.is_parameter(*node))
                || !self
                    .inputs
                    .iter()
                    .all(|node| graph.is_parameter(*node) || !graph.domain(*node).is_any())
            {
                return None;
            }
            seeds.extend(super::default_types());
        }
        let choices = facts.choices(seeds, !defaults);
        if choices.is_empty() {
            return None;
        }
        Some(choices)
    }

    fn select_inputs(&self, graph: &mut ConstraintGraph, facts: &mut CommonTypeFacts) -> usize {
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
        candidate
    }

    fn constrain_result(
        &self,
        graph: &mut ConstraintGraph,
        facts: &CommonTypeFacts,
        candidate: usize,
    ) {
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
    }

    fn constrain_coercions(&self, graph: &mut ConstraintGraph, facts: &CommonTypeFacts) {
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
    }
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::common_types as test_helpers;

    use crate::sem::parameter_constraints::{
        ConstraintGraph, InputTypeConstraint, ParameterConstraints, TypeProjection,
    };
    use crate::sem::{MinMaxExpr, PgBuiltInType, TypeReference};
    use std::collections::BTreeMap;

    #[rstest::rstest]
    fn sql_established_domain_is_not_broadened(
        #[values(false, true)] defaults: bool,
        #[values(false, true)] provisional: bool,
    ) {
        let context = test_helpers::inspection_context();
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let result = graph.common_type(&context, &[left, right], false, "CASE");
        drop(context);
        if provisional {
            graph.solve().unwrap();
            assert_eq!(graph.domain(left), &test_helpers::protocol_candidates());
        }
        let domain = InputTypeConstraint::exact(&"public.amount".parse().unwrap());
        graph.restrict(right, &domain);
        // A relation can acquire SQL context while deferred. Its witnesses
        // must obey that context even during the bounded-default phase.
        assert!(graph.instantiate_common_types(defaults));
        assert!(graph.propagate(false, &mut crate::sem::ConstraintMemo::default()));
        assert_eq!(graph.domain(left), &domain);
        assert_eq!(graph.domain(right), &domain);
        assert_eq!(graph.domain(result), &domain);
    }

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
        assert_eq!(graph.domain(left), &test_helpers::protocol_candidates());
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

    #[rstest::rstest]
    #[case::uuid("pg_catalog.uuid")]
    #[case::domain("public.amount")]
    #[case::array("pg_catalog.int4[]")]
    fn analyzed_case_retains_non_default_assignment_inspection(
        #[case] source: &str,
    ) -> testresult::TestResult {
        use crate::sem::ParseContext;
        let context = test_helpers::assignment_context();
        let mut cache = crate::test_helpers::create_cache_context();
        let analyzed = crate::parse_statements(
            "SELECT CASE WHEN true THEN $1 ELSE $2 END",
            crate::ast::Dialect::postgres(),
        )
        .next()
        .unwrap()?
        .analyze(context, &mut cache)?;
        let constraints = analyzed.context().parameter_constraints().clone();
        drop(analyzed);
        assert_eq!(
            constraints.parameter(1).unwrap().constraint(),
            &test_helpers::protocol_candidates()
        );
        let source: TypeReference = source.parse()?;
        assert!(constraints.allows(&BTreeMap::from([(1, source.clone()), (2, source.clone())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, source), (2, PgBuiltInType::text())])));
        Ok(())
    }

    #[rstest::rstest]
    #[case::uuid("pg_catalog.uuid")]
    #[case::domain("public.amount")]
    #[case::array("pg_catalog.int4[]")]
    #[case::domain_array("public.amount[]")]
    fn non_default_assignments_survive_solving_and_restoration(
        #[case] source: &str,
        #[values(false, true)] provisional: bool,
    ) {
        let context = test_helpers::inspection_context();
        let mut graph = ConstraintGraph::default();
        let left = graph.parameter(1);
        let right = graph.parameter(2);
        let output = graph.parameter(3);
        let result = graph.common_type(&context, &[left, right], false, "CASE");
        drop(context);
        if provisional {
            graph.solve().unwrap();
        }
        graph.equal(result, output);
        graph.solve().unwrap();
        let constraints = ParameterConstraints::from_graph(&graph);
        let source: TypeReference = source.parse().unwrap();
        assert!(constraints.allows(&BTreeMap::from([(1, source.clone())])));
        assert!(constraints.allows(&BTreeMap::from([
            (1, source.clone()),
            (2, source.clone()),
            (3, source.clone()),
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, source.clone()),
            (2, source),
            (3, PgBuiltInType::text()),
        ])));
    }

    #[rstest::rstest]
    #[case::domain_first("public.amount[]", "pg_catalog.int4[]", "public.amount[]", true)]
    #[case::wrong_result("public.amount[]", "pg_catalog.int4[]", "pg_catalog.int4[]", false)]
    #[case::base_first("pg_catalog.int4[]", "public.amount[]", "pg_catalog.int4[]", true)]
    #[case::wrong_reverse_result("pg_catalog.int4[]", "public.amount[]", "public.amount[]", false)]
    #[case::promoted("public.amount[]", "pg_catalog.int8[]", "pg_catalog.int8[]", true)]
    #[case::incompatible("public.amount[]", "pg_catalog.text[]", "public.amount[]", false)]
    fn deferred_domain_arrays_keep_input_result_correlations(
        #[case] left: &str,
        #[case] right: &str,
        #[case] result: &str,
        #[case] allowed: bool,
    ) {
        let mut graph = ConstraintGraph::default();
        let left_node = graph.parameter(1);
        let right_node = graph.parameter(2);
        let output = graph.parameter(3);
        {
            let context = test_helpers::domain_context();
            let node = graph.common_type(&context, &[left_node, right_node], false, "common type");
            graph.equal(node, output);
            graph.solve().unwrap();
        }
        // Instantiation and conversion checks now use only retained owned facts.
        let constraints = ParameterConstraints::from_graph(&graph);
        assert_eq!(
            constraints.allows(&BTreeMap::from([
                (1, left.parse().unwrap()),
                (2, right.parse().unwrap()),
                (3, result.parse().unwrap()),
            ])),
            allowed
        );
    }

    #[rstest::rstest]
    fn constructed_domain_arrays_keep_deferred_parameter_identity(
        #[values("public.amount", "public.amount[]", "public.nested_amount")] source: &str,
    ) {
        let mut graph = ConstraintGraph::default();
        let source_node = graph.parameter(1);
        let array = graph.node(&InputTypeConstraint::any());
        graph.project(source_node, array, TypeProjection::construct_array());
        let right = graph.parameter(2);
        let output = graph.parameter(3);
        {
            let context = test_helpers::domain_context();
            let result = graph.common_type(&context, &[array, right], false, "ARRAY");
            graph.equal(result, output);
            graph.solve().unwrap();
        }
        let constraints = ParameterConstraints::from_graph(&graph);
        let source: TypeReference = source.parse().unwrap();
        let mut assignment = BTreeMap::from([
            (1, source.clone()),
            (2, PgBuiltInType::int4().array_type()),
            (3, source.array_type()),
        ]);
        assert!(constraints.allows(&assignment));
        assignment.insert(3, PgBuiltInType::int4().array_type());
        assert!(!constraints.allows(&assignment));
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
