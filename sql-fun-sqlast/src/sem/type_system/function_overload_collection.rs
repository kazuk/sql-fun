use crate::sem::{
    OverloadVariant, ParseContext,
    type_system::{
        ArgumentBindingCollection, CommonInferContext, SimpleInferContext, TypeError,
        func_params::ParameterBindResult,
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
    pub(crate) fn view_row_properties(
        &self,
        name: &crate::sem::FullName,
        preserves_rows: bool,
    ) -> Vec<((crate::sem::FullName, OverloadVariant), bool)> {
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
    ) -> Result<(), TypeError> {
        let mut simple_infer_context = SimpleInferContext::default();
        for param in bind_result {
            param.collect_simple(&mut simple_infer_context)?;
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
        TParseContext: crate::sem::BaseParseContext,
    {
        let mut common_infer_context = CommonInferContext::default();
        for param in bind_result {
            param.collect_common(&mut common_infer_context)?;
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
        if !overload.is_variadic() {
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
        declarations: &[ParameterBindResult],
        context: &impl crate::sem::BaseParseContext,
    ) -> Option<crate::sem::overload_rank::OverloadRank> {
        let mut rank = crate::sem::overload_rank::OverloadRank::default();
        for (binding, declaration) in std::iter::zip(bindings, declarations) {
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

    pub(crate) fn resolve_from_catalog<TCatalog: crate::sem::BaseParseContext>(
        &self,
        context: &TCatalog,
        arg_types: &ArgumentBindingCollection,
    ) -> Option<OverloadVariant> {
        let mut candidates = Vec::new();

        'overload: for overload in &self.overloads {
            let mut overload = overload.clone();
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            if self.shadows_expanded_variadic(&overload, &bind_result, arg_types) {
                continue;
            }
            let declared_bindings = bind_result;
            let Ok(()) = Self::infer_simple(&declared_bindings, &mut overload) else {
                continue 'overload;
            };

            let Ok(()) = Self::infer_common(&declared_bindings, &mut overload, context) else {
                continue 'overload;
            };

            // re binding with inferred type
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            let Some(score) = Self::overload_rank(&bind_result, &declared_bindings, context) else {
                continue 'overload;
            };

            candidates.push((overload, score));
        }

        let best = candidates.iter().map(|(_, rank)| *rank).max()?;
        let mut best_candidates = candidates.into_iter().filter(|(_, rank)| *rank == best);
        let (overload, _) = best_candidates.next()?;
        if best_candidates.next().is_some() {
            None
        } else {
            Some(overload)
        }
    }
}

#[cfg(test)]
mod tests;

impl FunctionOverloadCollection {
    pub(crate) fn parameter_signatures(
        &self,
        catalog: &impl crate::sem::BaseParseContext,
        arguments: &[crate::sem::SemScalarExpr],
    ) -> Vec<crate::sem::parameter_resolution::Signature> {
        let arguments = ArgumentBindingCollection::from_expr_list(arguments);
        let mut signatures = Vec::new();
        for overload in &self.overloads {
            let mut overload = overload.clone();
            let Ok(bindings) = overload.bind_args(arguments.clone()) else {
                continue;
            };
            if self.shadows_expanded_variadic(&overload, &bindings, &arguments) {
                continue;
            }
            let declared_bindings = bindings;
            if Self::infer_simple(&declared_bindings, &mut overload).is_err()
                || Self::infer_common(&declared_bindings, &mut overload, catalog).is_err()
            {
                continue;
            }
            let Ok(bindings) = overload.bind_args(arguments.clone()) else {
                continue;
            };
            if let Some(signature) = overload.parameter_signature(&bindings, &declared_bindings) {
                signatures.push(signature);
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
    ) -> Option<crate::sem::parameter_resolution::Signature> {
        let inputs = ParameterBindResult::constraint_inputs(bindings)?;
        let declared_inputs = ParameterBindResult::constraint_inputs(declarations)?;
        crate::sem::parameter_resolution::Signature::new_function(
            &inputs,
            &declared_inputs,
            self.ret_type.as_ref()?,
        )
    }
}
