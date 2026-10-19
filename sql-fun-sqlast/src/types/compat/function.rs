//! Function catalog adaptation. Permanent signature operations live in `types`.

use super::{ConversionError, load_reference, polymorphic_position};
use crate::sem::{ColumnName, FullName, FunctionParam, OverloadVariant, ParseContext};
use crate::types::{
    DefaultExpression, FunctionOverloads, FunctionParameter, FunctionResult, FunctionSignature,
    ParameterMode, PolymorphicRole, PseudoType, SignaturePolymorphism, TypeReference,
};

/// Error location within a function's reported overload list.
#[derive(Debug, Clone, Eq, PartialEq, ::thiserror::Error)]
#[error("function {name}, overload {overload}, {component}: {reason}")]
pub struct FunctionLoadError {
    name: FullName,
    overload: usize,
    component: String,
    reason: String,
}

impl FunctionLoadError {
    /// Locate an unavailable or invalid component, using a zero-based overload index.
    pub fn new(name: &FullName, overload: usize, component: &str, reason: &str) -> Self {
        Self {
            name: name.clone(),
            overload,
            component: component.to_owned(),
            reason: reason.to_owned(),
        }
    }
    /// Function whose signature could not be loaded.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// Zero-based overload index in the provider's list.
    pub const fn overload(&self) -> usize {
        self.overload
    }
    /// Failing component path.
    pub fn component(&self) -> &str {
        &self.component
    }
}

pub(super) fn by_name<C: ParseContext>(
    context: &C,
    name: &FullName,
) -> Result<Option<FunctionOverloads>, ConversionError> {
    // Verify existence first: the legacy resolution helper falls back to the input name.
    if context.get_function_by_name(name).is_none() {
        return Ok(None);
    }
    by_exact_name(
        context,
        &context.resolve_function_name_with_search_path(name),
    )
}

pub(super) fn by_exact_name<C: ParseContext>(
    context: &C,
    name: &FullName,
) -> Result<Option<FunctionOverloads>, ConversionError> {
    let Some(overloads) = context.get_function_impl(name) else {
        return Ok(None);
    };
    let mut signatures = Vec::new();
    for (index, overload) in (&overloads).into_iter().enumerate() {
        signatures.push(load_signature(context, name, index, overload)?);
    }
    Ok(Some(FunctionOverloads::new(&signatures)))
}

const fn mode(parameter: &FunctionParam) -> ParameterMode {
    if parameter.is_variadic() {
        ParameterMode::variadic()
    } else if parameter.is_table_column() {
        ParameterMode::table()
    } else if parameter.accepts_input() && parameter.produces_output() {
        ParameterMode::in_out()
    } else if parameter.accepts_input() {
        ParameterMode::input()
    } else {
        ParameterMode::output()
    }
}

const fn role(parameter: &FunctionParam) -> PolymorphicRole {
    if parameter.is_variadic() {
        PolymorphicRole::variadic()
    } else if parameter.accepts_input() && parameter.produces_output() {
        PolymorphicRole::in_out()
    } else if parameter.accepts_input() {
        PolymorphicRole::input()
    } else {
        PolymorphicRole::output()
    }
}

fn load_signature<C: ParseContext>(
    context: &C,
    name: &FullName,
    index: usize,
    overload: &OverloadVariant,
) -> Result<FunctionSignature, ConversionError> {
    let error = |component: &str, reason: &str| {
        ConversionError::function(&FunctionLoadError::new(name, index, component, reason))
    };
    let metadata = overload
        .signature_metadata()
        .ok_or_else(|| error("declaration", "complete signature metadata unavailable"))?;
    let mut positions = Vec::new();
    let mut references = Vec::new();
    let mut links = Vec::new();
    for (i, parameter) in metadata.parameters().iter().enumerate() {
        let reference = parameter
            .get_type()
            .ok_or_else(|| error(&format!("parameter[{i}]"), "type unavailable"))?;
        let reference = resolved_reference(context, reference);
        links.push(polymorphic_link(
            context,
            &reference,
            role(parameter),
            &mut positions,
        ));
        references.push(reference);
    }
    let result_reference = metadata.result().map(|r| resolved_reference(context, r));
    let result_link = if let Some(reference) = &result_reference {
        polymorphic_link(
            context,
            reference,
            PolymorphicRole::output(),
            &mut positions,
        )
    } else {
        None
    };
    let polymorphism = SignaturePolymorphism::new(&positions)
        .map_err(|e| error("polymorphism", &e.to_string()))?;
    let mut parameters = Vec::new();
    let mut input = 0;
    for (i, parameter) in metadata.parameters().iter().enumerate() {
        let type_ = component(context, &references[i], links[i], &polymorphism)
            .map_err(|e| error(&format!("parameter[{i}]"), &e.to_string()))?;
        let default = if let Some(expression) = parameter.retained_default() {
            let snapshot = serde_json::to_value(expression)
                .map_err(|e| error(&format!("parameter[{i}].default"), &e.to_string()))?;
            Some(DefaultExpression::snapshot(
                "sql-fun-sem-expression-v1",
                &snapshot,
            ))
        } else if parameter.accepts_input() && input >= metadata.default_start() {
            metadata
                .default_sql()
                .map(|sql| DefaultExpression::sql_list(sql, input - metadata.default_start()))
        } else {
            None
        };
        parameters.push(FunctionParameter::new(
            &parameter.name().map(ColumnName::new),
            mode(parameter),
            type_.as_ref(),
            &default,
        ));
        if parameter.accepts_input() {
            input += 1;
        }
    }
    let result = if metadata.is_procedure() {
        if metadata.returns_set() || result_reference.is_some() {
            return Err(error("result", "inconsistent procedure result metadata"));
        }
        FunctionResult::procedure()
    } else {
        let reference =
            result_reference.ok_or_else(|| error("result", "declared result type unavailable"))?;
        let type_ = component(context, &reference, result_link, &polymorphism)
            .map_err(|e| error("result", &e.to_string()))?;
        if type_
            .as_any()
            .downcast_ref::<PseudoType>()
            .is_some_and(|p| p.name().is_record())
        {
            FunctionResult::record(type_.as_ref(), &parameters, metadata.returns_set())
        } else if metadata.returns_set() {
            FunctionResult::set(type_.as_ref())
        } else {
            FunctionResult::scalar(type_.as_ref())
        }
    };
    Ok(FunctionSignature::new(
        name,
        &parameters,
        &result,
        overload.is_strict(),
        overload.returns_not_null(),
    ))
}

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

#[cfg(test)]
mod tests;
