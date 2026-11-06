use crate::{
    context::BaseParseContext,
    sem::{
        CommonType, ParseContext, TypeReference,
        type_system::{
            TypeError,
            func_params::{FunctionParameterTrait, ParameterBindResult},
        },
    },
};

use std::collections::HashSet;

/// Simple-family substitution retains both element constraints and array identity.
pub struct SimpleInferResult {
    element: TypeReference,
    array: Option<TypeReference>,
}

impl SimpleInferResult {
    pub(crate) fn new(element: &TypeReference, array: Option<&TypeReference>) -> Self {
        Self {
            element: element.clone(),
            array: array.cloned(),
        }
    }

    pub(crate) fn apply(&self, target: &mut TypeReference) {
        if !target.is_dynamic()
            && target.is_any_array()
            && let Some(array) = &self.array
        {
            *target = array.clone();
        } else {
            target.apply_simple_infer_result(&self.element);
        }
    }
}

#[derive(Default)]
pub struct SimpleInferContext {
    require_infer: bool,
    element_types: HashSet<TypeReference>,
    array_types: HashSet<TypeReference>,
}

impl SimpleInferContext {
    /// insert candidate for anyelement
    fn push_element(&mut self, ty: &TypeReference) {
        self.element_types.insert(ty.clone());
        self.set_require_infer();
    }

    /// insert candidate for anyarray
    fn push_array(
        &mut self,
        array_ty: &TypeReference,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        if let Some(element) = catalog.get_array_item_type(array_ty) {
            self.array_types.insert(array_ty.clone());
            self.element_types.insert(element);
            self.set_require_infer();
            Ok(())
        } else {
            Err(TypeError::RequireArray(array_ty.clone()))
        }
    }

    fn push_non_array_element(
        &mut self,
        ty: &TypeReference,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        if catalog.get_array_item_type(ty).is_some() {
            Err(TypeError::RequireNonArray(ty.clone()))
        } else {
            self.push_element(ty);
            Ok(())
        }
    }

    fn push_range_element(&mut self, ty: &TypeReference) -> Result<(), TypeError> {
        if ty.is_range() {
            self.push_element(&ty.range_subtype());
            Ok(())
        } else {
            Err(TypeError::RequireRangeType(ty.clone()))
        }
    }

    fn push_multi_range_element(&mut self, ty: &TypeReference) -> Result<(), TypeError> {
        if ty.is_multi_range() {
            self.push_element(&ty.multi_range_base_element());
            Ok(())
        } else {
            Err(TypeError::RequireMultiRangeType(ty.clone()))
        }
    }

    /// insert type constraint
    pub fn push_input_param(
        &mut self,
        param_type: &TypeReference,
        arg_type: &TypeReference,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        if param_type.is_any_element() {
            self.push_element(arg_type);
        } else if param_type.is_any_array() {
            self.push_array(arg_type, catalog)?;
        } else if param_type.is_any_non_array() {
            self.push_non_array_element(arg_type, catalog)?;
        } else if param_type.is_any_range() {
            self.push_range_element(arg_type)?;
        } else if param_type.is_any_multi_range() {
            self.push_multi_range_element(arg_type)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test_simple_infer_context {
    use crate::test_helpers::TestParseContext;

    use crate::{
        names::FullName,
        sem::{
            TypeReference,
            type_system::{TypeError, infer::SimpleInferContext},
        },
    };

    use ::testresult::TestResult;

    #[test]
    fn test_simple_infer_minimum() -> TestResult {
        let mut context = SimpleInferContext::default();
        let param_type = TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int4"));
        let arg_type = TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int4"));

        context.push_input_param(&param_type, &arg_type, &TestParseContext::default())?;
        assert!(!context.is_require_infer());
        assert!(context.element_types.is_empty());
        assert!(matches!(
            context.get_inference_result(),
            Err(TypeError::SimpleInferNoArgs())
        ));

        let param_type =
            TypeReference::from_full_name(FullName::with_schema("pg_catalog", "anyelement"));
        assert!(param_type.is_simple_infer_arg());
        assert!(param_type.is_any_element());
        let arg_type = TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int4"));
        context.push_input_param(&param_type, &arg_type, &TestParseContext::default())?;

        assert!(context.is_require_infer());
        assert!(context.element_types.contains(&arg_type));

        let param_type =
            TypeReference::from_full_name(FullName::with_schema("pg_catalog", "anyarray"));
        assert!(param_type.is_simple_infer_arg());
        assert!(param_type.is_any_array());
        let arg_type =
            TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", "int4"), true);

        context.push_input_param(&param_type, &arg_type, &TestParseContext::default())?;
        assert!(context.is_require_infer());
        assert_eq!(context.element_types.len(), 1);
        assert!(context.get_inference_result().is_ok());

        let param_type =
            TypeReference::from_full_name(FullName::with_schema("pg_catalog", "anyelement"));
        assert!(param_type.is_simple_infer_arg());
        assert!(param_type.is_any_element());
        let arg_type = TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int8"));
        context.push_input_param(&param_type, &arg_type, &TestParseContext::default())?;
        assert_eq!(context.element_types.len(), 2);
        assert!(matches!(
            context.get_inference_result(),
            Err(TypeError::SimpleInferAmbiguous(_))
        ));

        Ok(())
    }
}

impl SimpleInferContext {
    /// get inference result
    pub fn get_inference_result(&self) -> Result<SimpleInferResult, TypeError> {
        if self.element_types.is_empty() {
            return Err(TypeError::SimpleInferNoArgs());
        }
        if self.element_types.len() != 1 {
            return Err(TypeError::simple_infer_ambiguous(&self.element_types));
        }
        if self.array_types.len() > 1 {
            return Err(TypeError::simple_infer_ambiguous(&self.array_types));
        }
        Ok(SimpleInferResult::new(
            self.element_types.iter().next().unwrap(),
            self.array_types.iter().next(),
        ))
    }

    pub const fn set_require_infer(&mut self) {
        self.require_infer = true;
    }

    pub const fn is_require_infer(&self) -> bool {
        self.require_infer
    }

    pub fn collect<TParam>(
        &mut self,
        param: &TParam,
        bind: &ParameterBindResult,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError>
    where
        TParam: FunctionParameterTrait + Sized,
    {
        if let Some(param_type) = param.param_type()
            && param_type.is_simple_infer_arg()
            && param.is_input_param()
        {
            for arg_type in bind.get_arg_types() {
                self.push_input_param(param_type, &arg_type, catalog)?;
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct CommonInferContext {
    require_infer: bool,
    require_non_array: bool,
    element_types: Vec<TypeReference>,
}

impl CommonInferContext {
    pub fn push_compat_element(&mut self, ty: &TypeReference) {
        self.element_types.push(ty.clone());
    }

    pub fn push_compat_array(
        &mut self,
        array_ty: &TypeReference,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        if *array_ty == CommonType::unknown() {
            self.push_compat_element(array_ty);
            Ok(())
        } else if let Some(element) = catalog.get_array_item_type(array_ty) {
            self.element_types.push(element);
            Ok(())
        } else {
            Err(TypeError::RequireArray(array_ty.clone()))
        }
    }

    pub fn push_compat_non_array(
        &mut self,
        ty: &TypeReference,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        if catalog.get_array_item_type(ty).is_some() {
            Err(TypeError::RequireNonArray(ty.clone()))
        } else {
            self.require_non_array = true;
            self.push_compat_element(ty);
            Ok(())
        }
    }

    fn push_compat_range_element(&mut self, ty: &TypeReference) -> Result<(), TypeError> {
        if ty.is_range() {
            self.push_compat_element(&ty.range_subtype());
            Ok(())
        } else {
            Err(TypeError::RequireRangeType(ty.clone()))
        }
    }

    fn push_compat_multi_range_element(&mut self, ty: &TypeReference) -> Result<(), TypeError> {
        if ty.is_multi_range() {
            self.push_compat_element(&ty.multi_range_base_element());
            Ok(())
        } else {
            Err(TypeError::RequireMultiRangeType(ty.clone()))
        }
    }

    /// insert type constraint
    pub fn push_input_param(
        &mut self,
        param_type: &TypeReference,
        arg_type: &TypeReference,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError> {
        if param_type.is_any_compatible() {
            self.push_compat_element(arg_type);
        } else if param_type.is_any_compatible_array() {
            self.push_compat_array(arg_type, catalog)?;
        } else if param_type.is_any_compatible_non_array() {
            self.push_compat_non_array(arg_type, catalog)?;
        } else if param_type.is_any_compatible_range() {
            self.push_compat_range_element(arg_type)?;
        } else if param_type.is_any_compatible_multi_range() {
            self.push_compat_multi_range_element(arg_type)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test_common_push_input_param {
    use crate::{
        names::FullName,
        sem::{
            TypeReference,
            type_system::{TypeError, infer::CommonInferContext},
        },
        test_helpers::TestParseContext,
    };

    use ::testresult::TestResult;

    #[test]
    fn push_input_param_requires_array_argument() -> TestResult {
        let mut context = CommonInferContext::default();
        let param_type = TypeReference::from_full_name(FullName::with_schema(
            "pg_catalog",
            "anycompatiblearray",
        ));
        let arg_type = TypeReference::from_master_type_name("int4");

        let result = context.push_input_param(&param_type, &arg_type, &TestParseContext::default());

        assert!(matches!(result, Err(TypeError::RequireArray(_))));
        assert!(context.element_types.is_empty());
        Ok(())
    }

    #[test]
    fn push_input_param_rejects_array_for_non_array_constraint() -> TestResult {
        let mut context = CommonInferContext::default();
        let param_type = TypeReference::from_full_name(FullName::with_schema(
            "pg_catalog",
            "anycompatiblenonarray",
        ));
        let arg_type = TypeReference::from_master_type_name("_int4");

        let result = context.push_input_param(&param_type, &arg_type, &TestParseContext::default());

        assert!(matches!(result, Err(TypeError::RequireNonArray(_))));
        assert!(context.element_types.is_empty());
        Ok(())
    }
}

impl CommonInferContext {
    /// gets common infer result
    pub fn get_inference_result<TParseContext>(
        &self,
        context: &TParseContext,
    ) -> Result<TypeReference, TypeError>
    where
        TParseContext: ParseContext,
    {
        self.infer_from_catalog(context)
    }
}

impl CommonInferContext {
    pub(crate) fn infer_from_catalog<TCatalog: BaseParseContext>(
        &self,
        context: &TCatalog,
    ) -> Result<TypeReference, TypeError> {
        let result = CommonType::select_common_type(context, &self.element_types)
            .map_err(|_| TypeError::CommonInferNoImplicitCastable())?;
        if self.require_non_array && context.get_array_item_type(&result).is_some() {
            return Err(TypeError::RequireNonArray(result));
        }
        Ok(result)
    }

    pub const fn set_require_infer(&mut self) {
        self.require_infer = true;
    }

    pub const fn is_require_infer(&self) -> bool {
        self.require_infer
    }

    pub fn collect<TParam>(
        &mut self,
        param: &TParam,
        bind: &ParameterBindResult,
        catalog: &impl BaseParseContext,
    ) -> Result<(), TypeError>
    where
        TParam: FunctionParameterTrait + Sized,
    {
        if let Some(param_type) = param.param_type()
            && param_type.is_common_infer_arg()
        {
            self.set_require_infer();
            if param.is_input_param() {
                for arg_type in bind.get_arg_types() {
                    self.push_input_param(param_type, &arg_type, catalog)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_common_get_inference_result {
    use crate::{
        sem::{PgBuiltInType, type_system::infer::CommonInferContext},
        test_helpers::common_types as test_helpers,
    };

    #[rstest::rstest]
    #[case::small_first(false)]
    #[case::large_first(true)]
    fn mixed_integers_infer_the_implicit_target(#[case] reverse: bool) {
        let mut context = CommonInferContext::default();
        let mut inputs = [PgBuiltInType::int4(), PgBuiltInType::int8()];
        if reverse {
            inputs.reverse();
        }
        for input in &inputs {
            context.push_compat_element(input);
        }
        assert_eq!(
            context
                .get_inference_result(&test_helpers::context())
                .unwrap(),
            PgBuiltInType::int8()
        );
    }
}
