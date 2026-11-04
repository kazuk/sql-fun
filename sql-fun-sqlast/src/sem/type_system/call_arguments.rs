//! Candidate-specific binding without changing the supplied operand order.

use crate::context::BaseParseContext;
use crate::sem::{
    FunctionParam, OverloadVariant,
    type_system::{ArgumentBindingCollection, TypeError, func_params::ParameterBindResult},
};

impl ArgumentBindingCollection {
    pub(crate) const fn with_explicit_variadic(mut self, explicit: bool) -> Self {
        self.explicit_variadic = explicit;
        self
    }
    pub(crate) fn has_call_notation(&self) -> bool {
        self.explicit_variadic || self.names.iter().any(Option::is_some)
    }
    pub(crate) const fn explicit_variadic(&self) -> bool {
        self.explicit_variadic
    }
    pub(crate) fn explicit_array_positions(
        &self,
        overload: &OverloadVariant,
    ) -> Option<Vec<usize>> {
        if !self.explicit_variadic {
            return Some(Vec::new());
        }
        Some(
            self.supplied_parameters(overload)
                .ok()?
                .into_iter()
                .enumerate()
                .filter_map(|(index, parameter)| {
                    if parameter.accepts_any_type() {
                        Some(index)
                    } else {
                        None
                    }
                })
                .collect(),
        )
    }
    pub(crate) fn valid_explicit_arrays(
        &self,
        context: &impl BaseParseContext,
        overload: &OverloadVariant,
    ) -> bool {
        self.explicit_array_positions(overload)
            .is_none_or(|positions| {
                positions
                    .into_iter()
                    .all(|index| match self.types.get(index) {
                        Some(Some(ty)) => context.get_array_item_type(ty).is_some(),
                        _ => true,
                    })
            })
    }
    pub(crate) fn supplied_parameters<'a>(
        &self,
        overload: &'a OverloadVariant,
    ) -> Result<Vec<&'a FunctionParam>, TypeError> {
        overload.supplied_parameters(self)
    }
}

impl OverloadVariant {
    fn supplied_parameters<'a>(
        &'a self,
        arguments: &ArgumentBindingCollection,
    ) -> Result<Vec<&'a FunctionParam>, TypeError> {
        let inputs: Vec<_> = self
            .params
            .iter()
            .filter(|parameter| parameter.is_input())
            .collect();
        let mut used = vec![false; inputs.len()];
        let mut named_seen = false;
        let mut supplied = Vec::new();
        for position in 0..arguments.types.len() {
            let name = if let Some(name) = arguments.names.get(position) {
                name.as_deref()
            } else {
                None
            };
            let index = if let Some(name) = name {
                named_seen = true;
                Some(
                    inputs
                        .iter()
                        .position(|parameter| parameter.name().map(String::as_str) == Some(name))
                        .ok_or_else(|| TypeError::UnknownArgumentName(name.to_owned()))?,
                )
            } else if named_seen {
                None
            } else {
                Some(position)
            };
            let Some(index) = index.filter(|index| *index < inputs.len()) else {
                return Err(TypeError::too_many_arguments());
            };
            if used[index] {
                return Err(TypeError::ArgumentAssignedTwice(
                    inputs[index].name().cloned().unwrap_or_default(),
                ));
            }
            if inputs[index].is_variadic() && !arguments.explicit_variadic {
                return Err(TypeError::too_many_arguments());
            }
            used[index] = true;
            supplied.push(inputs[index]);
        }
        for (parameter, used) in inputs.into_iter().zip(used) {
            if !used && parameter.default_expression().is_none() {
                return Err(TypeError::binding_failed(parameter));
            }
        }
        Ok(supplied)
    }
    pub(crate) fn has_omitted_inputs(&self, supplied: usize, explicit_variadic: bool) -> bool {
        self.params
            .iter()
            .filter(|parameter| {
                parameter.is_input() && (!parameter.is_variadic() || explicit_variadic)
            })
            .count()
            > supplied
    }
    pub(super) fn bind_supplied(
        &self,
        arguments: &ArgumentBindingCollection,
    ) -> Result<Vec<ParameterBindResult>, TypeError> {
        let parameters = arguments.supplied_parameters(self)?;
        let mut bindings = Vec::new();
        for (parameter, ty) in parameters.into_iter().zip(&arguments.types) {
            // An explicit VARIADIC operand is one array value, not expanded
            // element operands. The declaration itself remains unchanged.
            if parameter.accepts_any_type() {
                bindings.push(ParameterBindResult::supplied_variadic(
                    parameter,
                    ty.as_ref(),
                ));
                continue;
            }
            let target = parameter.get_type().cloned();
            let input = FunctionParam::new_input_param(&parameter.name().cloned(), &target, &None);
            bindings.push(ParameterBindResult::supplied(ty.as_ref(), &input));
        }
        Ok(bindings)
    }
}
