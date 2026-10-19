//! Shared legacy signature-component adaptation for functions and operators.

use super::{ConversionError, load_reference, polymorphic_position};
use crate::sem::ParseContext;
use crate::types::{PolymorphicRole, PseudoType, SignaturePolymorphism, TypeReference};

pub(super) fn resolved_reference<C: ParseContext>(
    context: &C,
    reference: &crate::sem::TypeReference,
) -> crate::sem::TypeReference {
    if reference.is_dynamic() {
        return reference.clone();
    }
    let name = reference.full_name();
    let resolved = context
        .resolve_type_name(name)
        .unwrap_or_else(|| name.clone());
    crate::sem::TypeReference::concrete_type_ref(&resolved, reference.is_array() == Some(true))
}

pub(super) fn polymorphic_link<C: ParseContext>(
    context: &C,
    reference: &crate::sem::TypeReference,
    role: PolymorphicRole,
    positions: &mut Vec<crate::types::PolymorphicPosition>,
) -> Option<usize> {
    if context.dialect() != crate::ast::Dialect::postgres() {
        return None;
    }
    let position = polymorphic_position(reference, role)?;
    let index = positions.len();
    positions.push(position);
    Some(index)
}

pub(super) fn component<C: ParseContext>(
    context: &C,
    reference: &crate::sem::TypeReference,
    link: Option<usize>,
    polymorphism: &SignaturePolymorphism,
) -> Result<TypeReference, ConversionError> {
    if let Some(link) = link {
        return Ok(Box::new(polymorphism.types()[link].clone()));
    }
    // These are declared pseudo categories, not missing concrete record layouts.
    if !reference.is_dynamic()
        && reference.is_array() == Some(false)
        && context.dialect() == crate::ast::Dialect::postgres()
    {
        let name = reference.full_name();
        if name.schema().is_some_and(|s| s.as_str() == "pg_catalog")
            && matches!(
                name.name().as_str(),
                "record"
                    | "void"
                    | "cstring"
                    | "any"
                    | "unknown"
                    | "trigger"
                    | "event_trigger"
                    | "internal"
                    | "language_handler"
                    | "fdw_handler"
                    | "tsm_handler"
                    | "table_am_handler"
                    | "index_am_handler"
                    | "pg_ddl_command"
                    | "opaque"
            )
        {
            return Ok(Box::new(PseudoType::new(name)));
        }
    }
    load_reference(context, reference)
}
