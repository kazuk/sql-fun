use crate::{
    context::{BaseParseContext, ParseContext},
    names::{ColumnName, FullName, TableName},
    sem::{
        ColumnCollection, OverloadVariant, TypeReference,
        overload_rank::OverloadRank,
        parameter_resolution::Signature,
        problem::CallArgumentProblem,
        type_system::{
            ArgumentBindingCollection, CommonInferContext, SimpleInferContext, TypeError,
            func_params::ParameterBindResult,
        },
    },
};

/// function overload variants defined in same name
#[derive(Debug, Clone, Default)]
pub struct FunctionOverloadCollection {
    overloads: Vec<OverloadVariant>,
}

impl<'a> IntoIterator for &'a FunctionOverloadCollection {
    type Item = &'a OverloadVariant;
    type IntoIter = std::slice::Iter<'a, OverloadVariant>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl FunctionOverloadCollection {
    /// Report a specific binding cause only when every candidate agrees.
    /// A bindable candidate suppresses binding diagnostics even if its types fail.
    pub(crate) fn binding_problem(
        &self,
        arguments: &ArgumentBindingCollection,
    ) -> Option<CallArgumentProblem> {
        let problems: Vec<_> = self
            .overloads
            .iter()
            .map(|overload| match arguments.supplied_parameters(overload) {
                Err(TypeError::UnknownArgumentName(name)) => {
                    Some(CallArgumentProblem::unknown_name(&name))
                }
                Err(TypeError::ArgumentAssignedTwice(name)) => {
                    Some(CallArgumentProblem::assigned_twice(&name))
                }
                Err(TypeError::BindFailed(parameter)) => Some(
                    CallArgumentProblem::missing_required(parameter.name().map(String::as_str)),
                ),
                _ => None,
            })
            .collect();
        let first = problems.first()?.as_ref()?;
        if problems
            .iter()
            .all(|problem| problem.as_ref() == Some(first))
        {
            Some(first.clone())
        } else {
            None
        }
    }

    // Shared declaration result, without binding or selecting an overload.
    pub(crate) fn common_scalar_result(&self) -> Option<TypeReference> {
        let result = self.overloads.first()?.scaler_ret_type()?;
        if result.is_simple_infer_arg() || result.is_common_infer_arg() {
            return None;
        }
        if self
            .overloads
            .iter()
            .all(|overload| overload.scaler_ret_type().as_ref() == Some(&result))
        {
            Some(result)
        } else {
            None
        }
    }

    pub(crate) fn returns_set(
        &self,
        context: &impl BaseParseContext,
        name: &FullName,
    ) -> Option<bool> {
        let first = context.function_returns_set(name, self.overloads.first()?);
        if self
            .overloads
            .iter()
            .all(|overload| context.function_returns_set(name, overload) == first)
        {
            Some(first)
        } else {
            None
        }
    }

    pub(crate) fn same_signatures(&self, other: &Self) -> bool {
        self.overloads == other.overloads
    }
    pub(crate) fn renamed_row_properties(
        &self,
        context: &impl BaseParseContext,
        name: &FullName,
        old: &TableName,
        new: &TableName,
    ) -> Vec<((FullName, OverloadVariant), bool)> {
        self.overloads
            .iter()
            .map(|signature| {
                let preserves = context.function_preserves_view_rows(name, signature);
                let mut renamed = signature.clone();
                renamed.rename_relation(old, new);
                ((name.clone(), renamed), preserves)
            })
            .collect()
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        for overload in &mut self.overloads {
            overload.rename_relation(old, new);
        }
    }

    pub(crate) fn view_row_properties(
        &self,
        name: &FullName,
        preserves_rows: bool,
    ) -> Vec<((FullName, OverloadVariant), bool)> {
        self.overloads
            .iter()
            .map(|overload| ((name.clone(), overload.clone()), preserves_rows))
            .collect()
    }

    pub(crate) fn record_definition_requirement(&self) -> Option<bool> {
        let requirement = self.overloads.first()?.requires_record_definition();
        if self
            .overloads
            .iter()
            .all(|overload| overload.requires_record_definition() == requirement)
        {
            Some(requirement)
        } else {
            None
        }
    }

    pub(crate) fn into_snapshot(self) -> Vec<OverloadVariant> {
        self.overloads
    }
    /// create new instance
    #[must_use]
    pub fn new(overloads: &[OverloadVariant]) -> Self {
        Self {
            overloads: overloads.to_vec(),
        }
    }

    /// append item
    pub fn push(&mut self, overload: &OverloadVariant) {
        self.overloads.push(overload.clone());
    }

    /// number of items
    #[must_use]
    pub const fn len(&self) -> usize {
        self.overloads.len()
    }

    /// Returns true if empty
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.overloads.is_empty()
    }

    /// get item from index
    #[must_use]
    pub fn get_at(&self, index: usize) -> Option<&OverloadVariant> {
        self.overloads.get(index)
    }

    /// iterate items
    pub fn iter(&self) -> std::slice::Iter<'_, OverloadVariant> {
        self.overloads.iter()
    }

    /// get merged collection
    #[must_use]
    pub fn merged(mut self, mut other: Self) -> Self {
        self.overloads.append(&mut other.overloads);
        self
    }

    /// insert overload variants
    pub fn append_overloads(&mut self, variants: &[OverloadVariant]) {
        for variant in variants {
            self.overloads.push(variant.clone());
        }
    }

    /// insert overload variants
    pub fn append(&mut self, variants: &Self) {
        for variant in &variants.overloads {
            self.overloads.push(variant.clone());
        }
    }

    fn infer_simple(
        bind_result: &Vec<ParameterBindResult>,
        overload: &mut OverloadVariant,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        let mut simple_infer_context = SimpleInferContext::default();
        for param in bind_result {
            param.collect_simple(&mut simple_infer_context, catalog)?;
        }
        if simple_infer_context.is_require_infer() {
            let inferred_type = simple_infer_context.get_inference_result()?;
            overload.apply_simple_infer_result(&inferred_type);
        }
        Ok(())
    }

    fn infer_common<TParseContext>(
        bind_result: &Vec<ParameterBindResult>,
        overload: &mut OverloadVariant,
        context: &TParseContext,
    ) -> Result<(), TypeError>
    where
        TParseContext: BaseParseContext,
    {
        let mut common_infer_context = CommonInferContext::default();
        for param in bind_result {
            param.collect_common(&mut common_infer_context, context)?;
        }
        if common_infer_context.is_require_infer() {
            let inferred_type = common_infer_context.infer_from_catalog(context)?;
            overload.apply_common_infer_result(&inferred_type);
        }
        Ok(())
    }

    // PostgreSQL prefers a non-variadic declaration over an expanded variadic
    // declaration with the same effective input signature in this collection.
    fn shadows_expanded_variadic(
        &self,
        overload: &OverloadVariant,
        bindings: &[ParameterBindResult],
        arguments: &ArgumentBindingCollection,
    ) -> bool {
        if !overload.is_variadic() || arguments.explicit_variadic() {
            return false;
        }
        let Some(inputs) = ParameterBindResult::constraint_inputs(bindings) else {
            return false;
        };
        self.overloads.iter().any(|other| {
            !other.is_variadic()
                && other.bind_args(arguments.clone()).is_ok_and(|bindings| {
                    ParameterBindResult::constraint_inputs(&bindings).as_ref() == Some(&inputs)
                })
        })
    }

    fn overload_rank(
        bindings: &[ParameterBindResult],
        literal_bindings: &[ParameterBindResult],
        declarations: &[ParameterBindResult],
        context: &impl BaseParseContext,
    ) -> Option<OverloadRank> {
        let mut rank = OverloadRank::default();
        for ((binding, literal), declaration) in
            std::iter::zip(bindings, literal_bindings).zip(declarations)
        {
            let binding = if declaration.is_common_argument() {
                literal
            } else {
                binding
            };
            rank = rank + binding.overload_rank(context, declaration)?;
        }
        Some(rank)
    }

    /// resolve variant for parameter types
    pub fn resolve_overload<TParseContext>(
        &self,
        context: &mut TParseContext,
        arg_types: &ArgumentBindingCollection,
    ) -> Option<OverloadVariant>
    where
        TParseContext: ParseContext,
    {
        self.resolve_from_catalog(context, arg_types)
    }

    pub(crate) fn resolve_from_catalog<TCatalog: BaseParseContext>(
        &self,
        context: &TCatalog,
        arg_types: &ArgumentBindingCollection,
    ) -> Option<OverloadVariant> {
        self.resolve_from_catalog_with_common_inputs(context, arg_types)
            .map(|(overload, _)| overload)
    }

    pub(crate) fn resolve_from_catalog_with_common_inputs<TCatalog: BaseParseContext>(
        &self,
        context: &TCatalog,
        arg_types: &ArgumentBindingCollection,
    ) -> Option<(OverloadVariant, Vec<bool>)> {
        let mut candidates = Vec::new();

        'overload: for overload in &self.overloads {
            let mut overload = overload.clone();
            if !arg_types.valid_explicit_arrays(context, &overload) {
                continue;
            }
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            if self.shadows_expanded_variadic(&overload, &bind_result, arg_types) {
                continue;
            }
            let declared_bindings = bind_result;
            let Ok(()) = Self::infer_simple(&declared_bindings, &mut overload, context) else {
                continue 'overload;
            };

            let Ok(common_bindings) = overload.bind_args(arg_types.common_literal_types()) else {
                continue 'overload;
            };
            let Ok(()) = Self::infer_common(&common_bindings, &mut overload, context) else {
                continue 'overload;
            };

            // re binding with inferred type
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            let Ok(literal_bindings) = overload.bind_args(arg_types.common_literal_rank_types())
            else {
                continue 'overload;
            };
            let Some(score) =
                Self::overload_rank(&bind_result, &literal_bindings, &declared_bindings, context)
            else {
                continue 'overload;
            };
            let common_inputs = ParameterBindResult::common_inputs(&declared_bindings);
            candidates.push((overload, score, common_inputs));
        }

        let best = candidates.iter().map(|(_, rank, _)| *rank).max()?;
        let mut best_candidates = candidates.into_iter().filter(|(_, rank, _)| *rank == best);
        let (overload, _, common_inputs) = best_candidates.next()?;
        if best_candidates.next().is_some() {
            None
        } else {
            Some((overload, common_inputs))
        }
    }
}

#[cfg(test)]
mod tests;

impl FunctionOverloadCollection {
    pub(crate) fn call_signatures(
        &self,
        catalog: &impl BaseParseContext,
        arguments: &ArgumentBindingCollection,
    ) -> Vec<Signature> {
        let mut signatures = Vec::new();
        for overload in &self.overloads {
            let mut overload = overload.clone();
            let Ok(bindings) = overload.bind_args(arguments.clone()) else {
                continue;
            };
            if self.shadows_expanded_variadic(&overload, &bindings, arguments) {
                continue;
            }
            let declared_bindings = bindings;
            if Self::infer_simple(&declared_bindings, &mut overload, catalog).is_err() {
                continue;
            }
            if arguments.types().any(|ty| ty.is_none()) {
                let Some(inputs) = ParameterBindResult::constraint_inputs(&declared_bindings)
                else {
                    continue;
                };
                if let Some(result) = overload.scaler_ret_type()
                    && let Some(template) =
                        Signature::compatible_function_template(&inputs, &result)
                {
                    signatures.push(template);
                    continue;
                }
            }
            if Self::infer_common(&declared_bindings, &mut overload, catalog).is_err() {
                continue;
            }
            let Ok(bindings) = overload.bind_args(arguments.clone()) else {
                continue;
            };
            if let Some(signature) = overload.parameter_signature(
                &bindings,
                &declared_bindings,
                arguments.has_call_notation(),
            ) {
                signatures.push(if arguments.has_call_notation() {
                    signature.with_simple_identity()
                } else {
                    signature
                });
            }
        }
        signatures
    }
}

impl OverloadVariant {
    fn parameter_signature(
        &self,
        bindings: &[ParameterBindResult],
        declarations: &[ParameterBindResult],
        allow_template: bool,
    ) -> Option<Signature> {
        let inputs = ParameterBindResult::constraint_inputs(bindings)?;
        let declared_inputs = ParameterBindResult::constraint_inputs(declarations)?;
        let signature = Signature::new_function(
            &inputs,
            &declared_inputs,
            &ParameterBindResult::unrestricted_inputs(bindings),
            self.ret_type.as_ref()?,
        );
        if signature.is_some() {
            signature
        } else if allow_template {
            Signature::simple_function_template(&inputs, &declared_inputs, self.ret_type.as_ref()?)
        } else {
            None
        }
    }
}

impl FunctionOverloadCollection {
    pub(crate) fn column_signatures(
        &self,
        context: &impl ParseContext,
        arguments: &ArgumentBindingCollection,
        scalar_name: &ColumnName,
        column: &ColumnName,
    ) -> Vec<Signature> {
        let overloads: Vec<_> = self
            .overloads
            .iter()
            .filter_map(|overload| {
                let columns = overload.table_function_output_columns(context, scalar_name);
                let ty = columns.get_column_def(column)?.get_type_cloned()?;
                let mut projection = overload.clone();
                projection.ret_type = Some(ty);
                Some(projection)
            })
            .collect();
        Self::new(&overloads).call_signatures(context, arguments)
    }

    pub(crate) fn common_table_output(
        &self,
        context: &impl ParseContext,
        name: &ColumnName,
    ) -> Option<ColumnCollection> {
        ColumnCollection::common_function_shape(
            &self
                .overloads
                .iter()
                .map(|overload| overload.table_function_output_columns(context, name))
                .collect::<Vec<_>>(),
        )
    }
}

impl FunctionOverloadCollection {
    pub(crate) fn explicit_array_positions(
        &self,
        arguments: &ArgumentBindingCollection,
    ) -> Vec<usize> {
        let mut intersection: Option<Vec<usize>> = None;
        for overload in &self.overloads {
            if let Some(positions) = arguments.explicit_array_positions(overload) {
                if let Some(previous) = &mut intersection {
                    previous.retain(|position| positions.contains(position));
                } else {
                    intersection = Some(positions);
                }
            }
        }
        intersection.unwrap_or_default()
    }
}
