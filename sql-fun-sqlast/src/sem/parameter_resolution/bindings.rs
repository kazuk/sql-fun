//! Application binding decisions, independent of statement-wide SQL admissibility.

use crate::{
    CacheContext,
    sem::{
        CommonType, InputTypeConstraint, ParseContext, SemScalarExpr, TypeReference,
        parameter_constraints::{TypeProjection, default_types, local_binding_domains},
        parameter_resolution::Signature,
        scalar_expr::ParamRef,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Default)]
pub(super) struct BindingSelections {
    occurrences: BTreeMap<(usize, u32), LocalBinding>,
    unlocated: BTreeSet<u32>,
}

#[derive(Default)]
enum LocalBinding {
    #[default]
    Unconstrained,
    Requirement {
        accepted: InputTypeConstraint,
        identity: InputTypeConstraint,
    },
    Call {
        call: Arc<LocalCall>,
        index: usize,
    },
    Projection {
        projection: TypeProjection,
        position: u32,
    },
}

struct LocalCall {
    arguments: Vec<LocalOperand>,
    signatures: Vec<Signature>,
}

enum LocalOperand {
    Parameter(u32),
    Expression(Option<TypeReference>),
}

impl LocalOperand {
    fn from_expression(value: &SemScalarExpr) -> Self {
        if let SemScalarExpr::Param(parameter) = value {
            Self::Parameter(parameter.position())
        } else {
            Self::Expression(value.get_type())
        }
    }

    fn candidates(
        &self,
        context: &impl ParseContext,
        selected: &BTreeMap<u32, InputTypeConstraint>,
        own: bool,
    ) -> InputTypeConstraint {
        match self {
            Self::Parameter(position) => {
                if let Some(hint) = context.get_parameter_typehint(*position as usize) {
                    if own {
                        return InputTypeConstraint::types(
                            default_types().into_iter().chain([hint]),
                        );
                    }
                    return InputTypeConstraint::exact(&hint);
                }
                if !own && let Some(selection) = selected.get(position) {
                    return selection.clone();
                }
                InputTypeConstraint::types(default_types())
            }
            Self::Expression(ty) => ty
                .as_ref()
                .map(InputTypeConstraint::exact)
                .unwrap_or_else(InputTypeConstraint::any),
        }
    }
}

impl LocalBinding {
    fn domains(
        &self,
        context: &impl ParseContext,
        cache: &mut CacheContext,
        selected: &BTreeMap<u32, InputTypeConstraint>,
    ) -> (InputTypeConstraint, InputTypeConstraint) {
        match self {
            Self::Unconstrained => (InputTypeConstraint::any(), InputTypeConstraint::types([])),
            Self::Projection {
                projection,
                position,
            } => {
                let mut projection = projection.clone();
                let candidates = default_types()
                    .into_iter()
                    .chain(context.get_parameter_typehint(*position as usize));
                let accepted = InputTypeConstraint::types(
                    candidates.filter(|source| projection.prepare(context, source).is_some()),
                );
                (accepted, InputTypeConstraint::types([]))
            }
            Self::Requirement { accepted, identity } => (accepted.clone(), identity.clone()),
            Self::Call { call, index } => {
                let inputs: Vec<_> = call
                    .arguments
                    .iter()
                    .enumerate()
                    .map(|(i, argument)| argument.candidates(context, selected, i == *index))
                    .collect();
                local_binding_domains(context, cache, &call.signatures, &inputs).remove(*index)
            }
        }
    }

    fn selection(
        &self,
        context: &impl ParseContext,
        cache: &mut CacheContext,
        selected: &BTreeMap<u32, InputTypeConstraint>,
    ) -> InputTypeConstraint {
        let (mut candidates, mut identity) = self.domains(context, cache, selected);
        candidates.intersect(&InputTypeConstraint::types(default_types()));
        identity.intersect(&candidates);
        if let Some(ty) = identity.single_type() {
            InputTypeConstraint::exact(&ty)
        } else if identity.is_empty() {
            candidates
        } else {
            identity
        }
    }
}

impl BindingSelections {
    pub(super) fn register(&mut self, parameter: &ParamRef) {
        if let Some(location) = parameter.location() {
            self.occurrences
                .entry((location.offset(), parameter.position()))
                .or_default();
        } else {
            self.unlocated.insert(parameter.position());
        }
    }

    // Only the nearest consuming operation supplies a local choice. Processing
    // follows SQL source order after collection, independently of query traversal.
    fn consume(&mut self, value: &SemScalarExpr, requirement: LocalBinding) {
        let SemScalarExpr::Param(parameter) = value else {
            return;
        };
        let Some(location) = parameter.location() else {
            return;
        };
        let Some(binding) = self
            .occurrences
            .get_mut(&(location.offset(), parameter.position()))
        else {
            return;
        };
        if matches!(binding, LocalBinding::Unconstrained) {
            *binding = requirement;
        }
    }

    pub(super) fn accept_target(
        &mut self,
        value: &SemScalarExpr,
        accepted: &InputTypeConstraint,
        target: &TypeReference,
    ) {
        self.consume(
            value,
            LocalBinding::Requirement {
                accepted: accepted.clone(),
                identity: InputTypeConstraint::exact(target),
            },
        );
    }

    pub(super) fn common_type(
        &mut self,
        context: &impl ParseContext,
        cache: &mut CacheContext,
        arguments: &[SemScalarExpr],
    ) {
        let known: Vec<_> = arguments
            .iter()
            .filter(|argument| !matches!(argument, SemScalarExpr::Param(_)))
            .filter_map(SemScalarExpr::common_type_input)
            .filter(|ty| *ty != CommonType::unknown())
            .collect();
        if known.is_empty() {
            return;
        }
        match CommonType::select_common_type(context, &known) {
            Ok(target) => {
                let sources =
                    InputTypeConstraint::types(cache.cast_sources(context, &target, false));
                for argument in arguments {
                    self.accept_target(argument, &sources, &target);
                }
            }
            Err(_) => {
                for argument in arguments {
                    self.consume(
                        argument,
                        LocalBinding::Requirement {
                            accepted: InputTypeConstraint::types([]),
                            identity: InputTypeConstraint::types([]),
                        },
                    );
                }
            }
        }
    }

    pub(super) fn project(&mut self, argument: &SemScalarExpr, projection: &TypeProjection) {
        if let SemScalarExpr::Param(parameter) = argument {
            self.consume(
                argument,
                LocalBinding::Projection {
                    projection: projection.clone(),
                    position: parameter.position(),
                },
            );
        }
    }

    pub(super) fn call(&mut self, arguments: &[SemScalarExpr], signatures: &[Signature]) {
        if !arguments
            .iter()
            .any(|argument| matches!(argument, SemScalarExpr::Param(_)))
        {
            return;
        }
        let call = Arc::new(LocalCall {
            arguments: arguments
                .iter()
                .map(LocalOperand::from_expression)
                .collect(),
            signatures: signatures.to_vec(),
        });
        for (index, argument) in arguments.iter().enumerate() {
            self.consume(
                argument,
                LocalBinding::Call {
                    call: call.clone(),
                    index,
                },
            );
        }
    }
}

impl BindingSelections {
    pub(super) fn selections(
        &self,
        context: &impl ParseContext,
        cache: &mut CacheContext,
    ) -> BTreeMap<u32, InputTypeConstraint> {
        // Missing source anchors cannot establish which use comes first.
        let mut selections: BTreeMap<_, _> = self
            .unlocated
            .iter()
            .map(|position| {
                let candidates =
                    if let Some(hint) = context.get_parameter_typehint(*position as usize) {
                        InputTypeConstraint::exact(&hint)
                    } else {
                        InputTypeConstraint::types(default_types())
                    };
                (*position, candidates)
            })
            .collect();
        for ((_, position), binding) in &self.occurrences {
            if !selections.contains_key(position) {
                let selection =
                    if let Some(hint) = context.get_parameter_typehint(*position as usize) {
                        InputTypeConstraint::exact(&hint)
                    } else {
                        binding.selection(context, cache, &selections)
                    };
                selections.insert(*position, selection);
            }
        }
        selections
    }

    pub(super) fn conflicts(
        &self,
        context: &impl ParseContext,
        cache: &mut CacheContext,
        selections: &BTreeMap<u32, InputTypeConstraint>,
    ) -> Vec<(u32, TypeReference, InputTypeConstraint)> {
        let mut conflicts = Vec::new();
        for ((_, position), binding) in &self.occurrences {
            if let Some(selection) = selections.get(position)
                && let Some(selected) = selection.single_type()
            {
                let (required, _) = binding.domains(context, cache, selections);
                if !required.allows(&selected) {
                    conflicts.push((*position, selected, required));
                }
            }
        }
        conflicts
    }
}

#[cfg(test)]
mod test_application_bindings {
    // These analysis-boundary contracts check that local binding decisions,
    // statement-wide admissibility and finalized operator diagnostics agree.
    use crate::{
        ast::Dialect,
        context::{ParameterTypeHintWrite, ParseContext},
        sem::{
            CastContext, CastDefinition, InputTypeConstraint, PgBuiltInType, SemAst, TypeReference,
            parameter_resolution::bindings::BindingSelections, scalar_expr::ParamRef,
        },
        test_helpers::{
            create_cache_context,
            public_analysis::{context, statement},
        },
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::cast("SELECT $1::text", None, Some("text"))]
    #[case::later("SELECT $1::text, $1::int4", None, Some("text"))]
    #[case::projection_before_source(
        "SELECT $1::text FROM (SELECT $1::int4) s",
        None,
        Some("text")
    )]
    #[case::cte_before_projection(
        "WITH s AS (SELECT $1::text) SELECT $1::int4 FROM s",
        None,
        Some("text")
    )]
    #[case::ambiguous_first("SELECT $1 IS NULL, $1::text", None, None)]
    #[case::hint("SELECT $1::text", Some("int4"), Some("int4"))]
    #[case::outside_defaults("SELECT $1 IS NULL", Some("bytea"), Some("bytea"))]
    #[case::unique_conversion("SELECT $1::bytea", None, Some("int4"))]
    #[case::operator_identity("SELECT $1 + 1", None, Some("int4"))]
    #[case::case_condition("SELECT CASE WHEN $1 THEN 1 ELSE 2 END", None, Some("bool"))]
    #[case::truth_test("SELECT $1 IS TRUE", None, Some("bool"))]
    #[case::hint_overrides_ambiguous_first(
        "SELECT $1 IS NULL, $1::text",
        Some("int4"),
        Some("int4")
    )]
    fn first_occurrence_or_supplied_hint_selects_binding(
        #[case] sql: &str,
        #[case] hint: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        let mut context = context();
        let bytea = TypeReference::from_master_type_name("bytea");
        context.setup_type(bytea.clone());
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &bytea,
            Some(CastDefinition::new(CastContext::explicit())),
        );
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &PgBuiltInType::text(),
            Some(CastDefinition::new(CastContext::explicit())),
        );
        context.set_get_implicit_cast_result(
            &PgBuiltInType::text(),
            &PgBuiltInType::int4(),
            Some(CastDefinition::new(CastContext::explicit())),
        );
        context.setup_typehinting(&[hint]).unwrap();
        let result = SemAst::from_ast(
            &statement(sql, Dialect::postgres()),
            context,
            &mut create_cache_context(),
        )
        .unwrap();
        let parameter = result
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap();
        assert_eq!(
            parameter.binding_type(),
            expected.map(TypeReference::from_master_type_name)
        );
        if expected.is_none() {
            assert_eq!(
                parameter
                    .binding_candidates()
                    .admissible_types()
                    .unwrap()
                    .len(),
                7
            );
        }
        assert!(
            result
                .context()
                .parameter_constraints()
                .allows(&BTreeMap::from([(
                    1,
                    TypeReference::from_master_type_name(hint.or(expected).unwrap_or("int4"))
                ),]))
        );
        if expected == Some("text") && hint.is_none() {
            assert!(
                result
                    .context()
                    .parameter_constraints()
                    .allows(&BTreeMap::from([(1, PgBuiltInType::int4()),]))
            );
        }
        assert_eq!(
            result.context().reported_problem_count(),
            0,
            "{:?}",
            result.context().reported_problems()
        );
    }

    #[rstest::rstest]
    #[case::unhinted(None)]
    #[case::hinted(Some("int4"))]
    fn missing_source_positions_do_not_invent_a_first_use(#[case] hint: Option<&str>) {
        let mut context = context();
        context.setup_typehinting(&[hint]).unwrap();
        let mut bindings = BindingSelections::default();
        bindings.register(&ParamRef::new(
            1,
            0,
            &InputTypeConstraint::exact(&PgBuiltInType::text()),
        ));
        let selected = bindings.selections(&context, &mut create_cache_context());
        assert_eq!(
            selected[&1].single_type(),
            hint.map(TypeReference::from_master_type_name)
        );
        if hint.is_none() {
            assert_eq!(selected[&1].admissible_types().unwrap().len(), 7);
        }
    }

    #[test]
    fn selected_binding_survives_later_operator_rejection() {
        let mut context = context();
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &PgBuiltInType::text(),
            Some(CastDefinition::new(CastContext::explicit())),
        );
        let result = SemAst::from_ast(
            &statement("SELECT $1::text, $1 + 1", Dialect::postgres()),
            context,
            &mut create_cache_context(),
        )
        .unwrap();
        assert_eq!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .binding_type(),
            Some(PgBuiltInType::text())
        );
        assert!(
            result
                .context()
                .has_reported_problem("binary_operator_overload_resolution_failed"),
            "{:?}",
            result.context().reported_problems()
        );
        assert!(
            !result
                .context()
                .has_reported_problem("parameter_type_unresolved")
        );
    }
}
