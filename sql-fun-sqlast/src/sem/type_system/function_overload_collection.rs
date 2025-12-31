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

impl FunctionOverloadCollection {
    /// create new instance
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
    pub fn len(&self) -> usize {
        self.overloads.len()
    }

    /// Returns true if empty
    pub fn is_empty(&self) -> bool {
        self.overloads.is_empty()
    }

    /// get item from index
    pub fn get_at(&self, index: usize) -> Option<&OverloadVariant> {
        self.overloads.get(index)
    }

    /// iterate items
    pub fn iter(&self) -> std::slice::Iter<'_, OverloadVariant> {
        self.overloads.iter()
    }

    /// get merged collection
    pub fn merged(mut self, mut other: Self) -> Self {
        self.overloads.append(&mut other.overloads);
        self
    }

    /// insert overload variants
    pub fn append_overloads(&mut self, variants: &[OverloadVariant]) {
        for variant in variants {
            self.overloads.push(variant.clone())
        }
    }

    /// insert overload variants
    pub fn append(&mut self, variants: &Self) {
        for variant in &variants.overloads {
            self.overloads.push(variant.clone())
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
            let infered_type = simple_infer_context.get_inference_result()?;
            overload.apply_simple_infer_result(&infered_type)
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
            let infered_type = common_infer_context.get_inference_result(context)?;
            overload.apply_common_infer_result(&infered_type);
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
        let mut candiates = Vec::new();

        'overload: for overload in &self.overloads {
            let mut overload = overload.clone();
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            let Ok(_) = Self::infer_simple(&bind_result, &mut overload) else {
                continue 'overload;
            };

            let Ok(_) = Self::infer_common(&bind_result, &mut overload, context) else {
                continue 'overload;
            };

            // re binding with infered type
            let Ok(bind_result) = overload.bind_args(arg_types.clone()) else {
                continue 'overload;
            };

            let Ok(score) = Self::test_castability(&bind_result, context) else {
                continue 'overload;
            };

            candiates.push((overload, score));
        }

        if candiates.len() == 1 {
            // candiates[0]
            Some(candiates.into_iter().next().unwrap().0)
        } else {
            // multiple overload found, then ambigous
            None
        }
    }
}
