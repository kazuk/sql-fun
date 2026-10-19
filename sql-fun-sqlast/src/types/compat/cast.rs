//! Direct cast adaptation and candidate loading through the selected catalog.

use crate::sem::{
    CastContext as LegacyContext, CastDefinition as LegacyCast, ParseContext,
    TypeReference as LegacyReference,
};
use crate::types::{Candidates, CastContext, CastDefinition, TypeReference};

use super::{ConversionError, load_reference};

impl From<LegacyContext> for CastContext {
    fn from(value: LegacyContext) -> Self {
        match value {
            LegacyContext::ImplicitPrefeerd => Self::implicit_preferred(),
            LegacyContext::Implicit => Self::implicit(),
            LegacyContext::Assignment => Self::assignment(),
            LegacyContext::Explicit => Self::explicit(),
            LegacyContext::NoConversion => Self::no_conversion(),
        }
    }
}

/// Adapt an existing cast and its explicitly supplied endpoint references.
///
/// Both endpoints are loaded completely through the selected context. This
/// operation preserves the supplied metadata; it does not check cast eligibility
/// or infer an endpoint from the cast context.
pub fn load_cast_definition<C: ParseContext>(
    context: &C,
    source: &LegacyReference,
    target: &LegacyReference,
    definition: &LegacyCast,
) -> Result<CastDefinition, ConversionError> {
    let source = load_reference(context, source)?;
    let target = load_reference(context, target)?;
    Ok(CastDefinition::new(
        source.as_ref(),
        target.as_ref(),
        &CastContext::from(definition.context()),
    ))
}

/// Load a direct implicit cast, including identity, from the selected catalog.
///
/// Concrete names are resolved with the context's search path before lookup;
/// array qualification is preserved. The lookup uses those same resolved
/// identities as endpoint loading. This differs from passing raw unqualified
/// names to legacy cast APIs. Dynamic references retain legacy lookup behavior.
///
/// Both endpoints must load completely, even when no relationship exists.
/// `Ok(None)` means that the selected catalog reports no direct cast between
/// these valid loaded endpoints. No cast-chain search or inference is performed.
pub fn load_implicit_cast<C: ParseContext>(
    context: &C,
    source: &LegacyReference,
    target: &LegacyReference,
) -> Result<Option<CastDefinition>, ConversionError> {
    load_cast(context, source, target, C::get_implicit_cast)
}

/// Load a direct assignment cast with the endpoint/error contract of
/// [`load_implicit_cast`]. Uses the catalog's assignment-specific lookup,
/// including its existing fallback behavior.
pub fn load_assignment_cast<C: ParseContext>(
    context: &C,
    source: &LegacyReference,
    target: &LegacyReference,
) -> Result<Option<CastDefinition>, ConversionError> {
    load_cast(context, source, target, C::get_assignment_cast)
}

/// Load a direct explicit cast with the endpoint/error contract of
/// [`load_implicit_cast`], preserving the catalog's returned context rather than
/// replacing it with the requested explicit mode.
pub fn load_explicit_cast<C: ParseContext>(
    context: &C,
    source: &LegacyReference,
    target: &LegacyReference,
) -> Result<Option<CastDefinition>, ConversionError> {
    load_cast(context, source, target, C::get_explicit_cast)
}

fn load_cast<C: ParseContext>(
    context: &C,
    source: &LegacyReference,
    target: &LegacyReference,
    lookup: fn(&C, &LegacyReference, &LegacyReference) -> Option<LegacyCast>,
) -> Result<Option<CastDefinition>, ConversionError> {
    let source = resolve_reference(context, source)?;
    let target = resolve_reference(context, target)?;
    let loaded_source = load_reference(context, &source)?;
    let loaded_target = load_reference(context, &target)?;
    let Some(definition) = lookup(context, &source, &target) else {
        return Ok(None);
    };
    Ok(Some(CastDefinition::new(
        loaded_source.as_ref(),
        loaded_target.as_ref(),
        &CastContext::from(definition.context()),
    )))
}

/// Load the source-type candidates enumerated by the selected legacy catalog.
///
/// `explicit` explicitly selects explicit eligibility (`true`) or implicit
/// eligibility (`false`), as in `CastInfoRead::cast_sources`. The target is
/// resolved and validated separately; the returned value describes candidate
/// inputs, not the cast expression's target/result type. Enumeration completeness
/// is exactly that of the selected legacy implementation, with no additional
/// cast-chain search.
///
/// Every enumerated source must load completely. An empty successful enumeration
/// constructs `Never`; missing/incomplete catalog data returns an error, never
/// a partial candidate collection or `Never`. Identity is retained when the
/// catalog enumerates it. A singleton remains `Candidates`.
pub fn load_cast_sources<C: ParseContext>(
    context: &C,
    target: &LegacyReference,
    explicit: bool,
) -> Result<TypeReference, ConversionError> {
    let target = resolve_reference(context, target)?;
    load_reference(context, &target)?;
    let mut candidates = Vec::new();
    for source in context.cast_sources(&target, explicit) {
        candidates.push(load_reference(context, &source)?);
    }
    Ok(Candidates::from_candidates(&candidates))
}

/// Load assignment-source candidates using the catalog's assignment enumeration.
///
/// The endpoint validation, complete-loading and empty-result contracts are the
/// same as [`load_cast_sources`]. Assignment enumeration remains distinct from
/// explicit eligibility.
pub fn load_assignment_sources<C: ParseContext>(
    context: &C,
    target: &LegacyReference,
) -> Result<TypeReference, ConversionError> {
    let target = resolve_reference(context, target)?;
    load_reference(context, &target)?;
    let mut candidates = Vec::new();
    for source in context.assignment_sources(&target) {
        candidates.push(load_reference(context, &source)?);
    }
    Ok(Candidates::from_candidates(&candidates))
}

/// Load the target-type candidates enumerated as implicitly castable from a source.
///
/// Uses the selected catalog's `get_implicit_castable` operation directly, with
/// the same complete-loading and empty-result contract as [`load_cast_sources`].
/// The resulting candidates describe output types, not accepted source types.
pub fn load_implicit_castable<C: ParseContext>(
    context: &C,
    source: &LegacyReference,
) -> Result<TypeReference, ConversionError> {
    let source = resolve_reference(context, source)?;
    load_reference(context, &source)?;
    let mut candidates = Vec::new();
    for target in context.get_implicit_castable(&source) {
        candidates.push(load_reference(context, &target)?);
    }
    Ok(Candidates::from_candidates(&candidates))
}

fn resolve_reference<C: ParseContext>(
    context: &C,
    reference: &LegacyReference,
) -> Result<LegacyReference, ConversionError> {
    match reference {
        LegacyReference::Concrete(value) => {
            let name = context
                .resolve_type_name(value.full_name())
                .ok_or_else(|| ConversionError::type_not_found(value.full_name()))?;
            Ok(LegacyReference::concrete_type_ref(&name, value.is_array()))
        }
        LegacyReference::Dynamic(_) => Ok(reference.clone()),
    }
}
