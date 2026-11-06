//! PostgreSQL common-type selection and coercion shared by SQL consumers.

use crate::{
    context::ParseContext,
    sem::{PgBuiltInType, TypeReference, problem::MinMaxProblem},
};

pub(crate) struct CommonType;

impl CommonType {
    pub(crate) fn unknown() -> TypeReference {
        TypeReference::from_master_type_name("unknown")
    }

    pub(crate) fn base_type(context: &impl ParseContext, ty: &TypeReference) -> TypeReference {
        // An array of domains is an array type, not itself a domain.
        if ty.is_array() == Some(true) {
            return ty.clone();
        }
        let mut base = ty.clone();
        let mut seen = std::collections::HashSet::new();
        while !base.is_dynamic() && base.is_array() != Some(true) && seen.insert(base.clone()) {
            let Some(definition) = context.get_type_impl(
                base.full_name()
                    .expect("owned definitions are handled before nominal lookup"),
            ) else {
                break;
            };
            let Some(reference) = definition.domain_base() else {
                break;
            };
            base = reference.clone();
        }
        base
    }

    pub(crate) fn category(
        context: &impl ParseContext,
        ty: &TypeReference,
    ) -> Result<(char, bool), MinMaxProblem> {
        if ty.is_array() == Some(true) {
            return Ok(('A', false));
        }
        let definition = if let Some(definition) = ty.type_definition() {
            Some(definition)
        } else {
            context.get_type_impl(
                ty.full_name()
                    .expect("owned definitions are handled before nominal lookup"),
            )
        };
        let category = if let Some(definition) = definition {
            definition.category(context)
        } else {
            None
        };
        let category = category.ok_or_else(|| MinMaxProblem::missing_type_facts(ty))?;
        if ty.is_dynamic() {
            return Ok((category.code(), false));
        }
        // Preference originates in a declared conversion. Domain-source
        // reductions preserve that declaration's target preference; discovering
        // every nominal domain cannot add a preferred target.
        let preferred = context
            .declared_implicit_cast_sources(ty)
            .into_iter()
            .any(|source| {
                context
                    .get_implicit_cast(&source, ty)
                    .is_some_and(|cast| cast.is_implicit_preferred())
            });
        Ok((category.code(), preferred))
    }

    pub(crate) fn can_coerce(
        context: &impl ParseContext,
        source: &TypeReference,
        target: &TypeReference,
    ) -> bool {
        if source == target || *source == Self::unknown() {
            return true;
        }
        // Coercion into a domain uses its base type too. This checks conversion
        // applicability without replacing the nominal common-type candidate.
        let source = Self::base_type(context, source);
        let target = Self::base_type(context, target);
        if source == target || source.implicit_match_context(&target, context).is_some() {
            return true;
        }
        if source.is_array() == Some(true) && target.is_array() == Some(true) {
            return Self::can_coerce(context, &source.element_type(), &target.element_type());
        }
        false
    }

    pub(crate) fn next_candidate(
        context: &impl ParseContext,
        candidate: &TypeReference,
        input: &TypeReference,
    ) -> Result<TypeReference, MinMaxProblem> {
        if *candidate == Self::unknown() {
            return Ok(input.clone());
        }
        if *input == Self::unknown() {
            return Ok(candidate.clone());
        }
        Self::select_candidate(
            candidate,
            input,
            Self::category(context, candidate)?,
            Self::category(context, input)?,
            Self::can_coerce(context, candidate, input),
            Self::can_coerce(context, input, candidate),
        )
    }

    pub(crate) fn select_candidate(
        candidate: &TypeReference,
        input: &TypeReference,
        (category, preferred): (char, bool),
        (input_category, _): (char, bool),
        forward: bool,
        backward: bool,
    ) -> Result<TypeReference, MinMaxProblem> {
        if category != input_category {
            return Err(MinMaxProblem::incompatible(candidate, input));
        }
        if !preferred && forward && !backward {
            return Ok(input.clone());
        }
        Ok(candidate.clone())
    }

    /// PostgreSQL ordered common-type selection, without comparison requirements.
    pub(crate) fn select_common_type(
        context: &impl ParseContext,
        inputs: &[TypeReference],
    ) -> Result<TypeReference, MinMaxProblem> {
        let Some(first) = inputs.first() else {
            return Err(MinMaxProblem::empty());
        };
        let target = if *first != Self::unknown() && inputs.iter().all(|ty| ty == first) {
            first.clone()
        } else {
            let mut candidate = Self::unknown();
            for input in inputs {
                candidate =
                    Self::next_candidate(context, &candidate, &Self::base_type(context, input))?;
            }
            if candidate == Self::unknown() {
                PgBuiltInType::text()
            } else {
                candidate
            }
        };
        for input in inputs {
            if !Self::can_coerce(context, input, &target) {
                return Err(MinMaxProblem::incompatible(input, &target));
            }
        }
        Ok(target)
    }
}
