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
        TParseContext: ParseContext,
    {
        let mut common_infer_context = CommonInferContext::default();
        for param in bind_result {
            param.collect_common(&mut common_infer_context)?;
        }
        if common_infer_context.is_require_infer() {
            let inferred_type = common_infer_context.get_inference_result(context)?;
            overload.apply_common_infer_result(&inferred_type);
        }
        Ok(())
    }

    fn test_castability<TParseContext>(
        bind_result: &Vec<ParameterBindResult>,
        context: &TParseContext,
    ) -> Result<i32, TypeError>
    where
        TParseContext: ParseContext,
    {
        let mut total_score = 0i32;
        for param in bind_result {
            total_score += param.test_castability(context)?;
        }
        Ok(total_score)
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
        let mut candidates = Vec::new();

        'overload: for overload in &self.overloads {
            let mut overload = overload.clone();
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            let Ok(()) = Self::infer_simple(&bind_result, &mut overload) else {
                continue 'overload;
            };

            let Ok(()) = Self::infer_common(&bind_result, &mut overload, context) else {
                continue 'overload;
            };

            // re binding with inferred type
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            let Ok(score) = Self::test_castability(&bind_result, context) else {
                continue 'overload;
            };

            candidates.push((overload, score));
        }

        if candidates.len() == 1 {
            // candidates[0]
            candidates.into_iter().next().map(|(overload, _)| overload)
        } else {
            // multiple overload found, then ambiguous
            None
        }
    }
}
