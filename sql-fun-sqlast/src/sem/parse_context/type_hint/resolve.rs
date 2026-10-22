//! Construct named type references directly, without a statement or expression AST.
use crate::ast::Dialect;
use crate::sem::{AnalysisError, ParseContext, TypeReference, parse_client_type};

pub(super) fn resolve_parameter_hint(
    slot: usize,
    hint: &str,
    context: &impl ParseContext,
) -> Result<Option<TypeReference>, AnalysisError> {
    let reference = parse_client_type(hint, context).map_err(|error| {
        AnalysisError::unexpected_input_error(&format!(
            "parameter ${} type hint {hint:?}: {error}",
            slot + 1,
        ))
    })?;
    let Some(canonical) = context.canonicalize_type_ref(&reference) else {
        return Err(AnalysisError::unexpected_input_error(&format!(
            "parameter ${} type hint {hint:?}: SQL type {reference} not found",
            slot + 1,
        )));
    };
    // PostgreSQL treats an explicitly declared unknown parameter as unspecified.
    if context.dialect() == Dialect::postgres()
        && canonical == TypeReference::from_master_type_name("unknown")
    {
        Ok(None)
    } else {
        Ok(Some(canonical))
    }
}
