//! Legacy operator catalog adaptation; permanent operations live in `types`.

use super::ConversionError;
use super::function::{component, polymorphic_link, resolved_reference};
use crate::sem::{
    OperatorDefinition, OperatorName, ParseContext, TypeReference as LegacyReference,
};
use crate::types::{
    OperatorForm, OperatorOverloads, OperatorSignature, PolymorphicRole, SignaturePolymorphism,
};

/// Location of an unavailable component in a reported operator overload list.
#[derive(Debug, Clone, Eq, PartialEq, ::thiserror::Error)]
#[error("operator {name:?}, {form}, overload {overload}, {component}: {reason}")]
pub struct OperatorLoadError {
    name: OperatorName,
    form: OperatorForm,
    overload: usize,
    component: String,
    reason: String,
}

impl OperatorLoadError {
    /// Locate an unavailable component, with a zero-based provider overload index.
    pub fn new(
        name: &OperatorName,
        form: OperatorForm,
        overload: usize,
        component: &str,
        reason: &str,
    ) -> Self {
        Self {
            name: name.clone(),
            form,
            overload,
            component: component.to_owned(),
            reason: reason.to_owned(),
        }
    }
    /// Explicit lookup name/key; no missing schema provenance is synthesized.
    pub const fn name(&self) -> &OperatorName {
        &self.name
    }
    /// Requested syntactic form.
    pub const fn form(&self) -> OperatorForm {
        self.form
    }
    /// Zero-based index in the provider's overload vector.
    pub const fn overload(&self) -> usize {
        self.overload
    }
    /// Failing component path.
    pub fn component(&self) -> &str {
        &self.component
    }
    /// Explanation from the component loader or catalog-contract check.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

pub(super) fn by_name<C: ParseContext>(
    context: &C,
    name: &OperatorName,
    form: OperatorForm,
) -> Result<Option<OperatorOverloads>, ConversionError> {
    let Some(definition) = context.get_operator_op(name, form.has_left(), form.has_right()) else {
        return Ok(None);
    };
    let mut signatures = Vec::new();
    match definition {
        OperatorDefinition::Binary(overloads) => {
            require_form(name, form, OperatorForm::binary())?;
            for (index, overload) in overloads.iter().enumerate() {
                signatures.push(load_signature(
                    context,
                    name,
                    form,
                    index,
                    &[
                        overload.left_arg_type(),
                        overload.right_arg_type(),
                        overload.get_return_type(),
                    ],
                    overload.is_strict(),
                )?);
            }
        }
        OperatorDefinition::LeftUnary(overloads) => {
            require_form(name, form, OperatorForm::left_unary())?;
            for (index, overload) in overloads.iter().enumerate() {
                signatures.push(load_signature(
                    context,
                    name,
                    form,
                    index,
                    &[overload.arg_type(), overload.get_return_type()],
                    overload.is_strict(),
                )?);
            }
        }
        OperatorDefinition::RightUnary(overloads) => {
            require_form(name, form, OperatorForm::right_unary())?;
            for (index, overload) in overloads.iter().enumerate() {
                signatures.push(load_signature(
                    context,
                    name,
                    form,
                    index,
                    &[overload.arg_type(), overload.get_return_type()],
                    overload.is_strict(),
                )?);
            }
        }
    }
    Ok(Some(OperatorOverloads::new(&signatures)))
}

fn require_form(
    name: &OperatorName,
    requested: OperatorForm,
    actual: OperatorForm,
) -> Result<(), ConversionError> {
    if requested == actual {
        Ok(())
    } else {
        Err(ConversionError::operator(&OperatorLoadError::new(
            name,
            requested,
            0,
            "form",
            "provider returned a different operator form",
        )))
    }
}

fn load_signature<C: ParseContext>(
    context: &C,
    name: &OperatorName,
    form: OperatorForm,
    index: usize,
    references: &[&LegacyReference],
    strict: bool,
) -> Result<OperatorSignature, ConversionError> {
    let error = |component: &str, reason: &str| {
        ConversionError::operator(&OperatorLoadError::new(
            name, form, index, component, reason,
        ))
    };
    let mut positions = Vec::new();
    let mut resolved = Vec::new();
    let mut links = Vec::new();
    for (i, reference) in references.iter().enumerate() {
        let reference = resolved_reference(context, reference);
        let role = if i + 1 == references.len() {
            PolymorphicRole::output()
        } else {
            PolymorphicRole::input()
        };
        links.push(polymorphic_link(context, &reference, role, &mut positions));
        resolved.push(reference);
    }
    let polymorphism = SignaturePolymorphism::new(&positions)
        .map_err(|e| error("polymorphism", &e.to_string()))?;
    let mut loaded = Vec::new();
    for (i, reference) in resolved.iter().enumerate() {
        let path = if i + 1 == references.len() {
            "result"
        } else if i == 0 && form.has_left() {
            "left_operand"
        } else {
            "right_operand"
        };
        loaded.push(
            component(context, reference, links[i], &polymorphism)
                .map_err(|e| error(path, &e.to_string()))?,
        );
    }
    if form == OperatorForm::binary() {
        Ok(OperatorSignature::binary(
            name,
            loaded[0].as_ref(),
            loaded[1].as_ref(),
            loaded[2].as_ref(),
            strict,
        ))
    } else if form == OperatorForm::left_unary() {
        Ok(OperatorSignature::left_unary(
            name,
            loaded[0].as_ref(),
            loaded[1].as_ref(),
            strict,
        ))
    } else {
        Ok(OperatorSignature::right_unary(
            name,
            loaded[0].as_ref(),
            loaded[1].as_ref(),
            strict,
        ))
    }
}

#[cfg(test)]
mod tests;
