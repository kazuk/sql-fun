//! Construct named type references directly, without a statement or expression AST.
use crate::ast::Dialect;
use crate::sem::{AnalysisError, ParseContext, TypeReference};

pub(super) fn resolve_parameter_hint(
    hint: &str,
    context: &impl ParseContext,
) -> Result<Option<TypeReference>, AnalysisError> {
    let reference = hint.parse::<TypeReference>()?;
    let canonical = context.canonicalize_type_ref(&reference).ok_or_else(|| {
        AnalysisError::unexpected_input_error(&format!("SQL type {reference} not found"))
    })?;
    // PostgreSQL treats an explicitly declared unknown parameter as unspecified.
    if context.dialect() == Dialect::postgres()
        && canonical == TypeReference::from_master_type_name("unknown")
    {
        Ok(None)
    } else {
        Ok(Some(canonical))
    }
}
