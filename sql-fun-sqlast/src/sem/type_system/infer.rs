use std::collections::HashSet;

use sql_fun_core::IVec;

use crate::sem::{
    ParseContext, TypeReference,
    type_system::{
        TypeError,
        func_params::{FunctionParameterTrait, ParameterBindResult},
    },
};

#[derive(Default)]
pub struct SimpleInferContext {
    require_infer: bool,
    element_types: HashSet<TypeReference>,
}

impl SimpleInferContext {
    /// insert candiate for anyelement
    fn push_element(&mut self, ty: &TypeReference) {
        self.element_types.insert(ty.clone());
        self.set_require_infer();
    }

    /// insert candiate for anyarray
    fn push_array(&mut self, array_ty: &TypeReference) -> Result<(), TypeError> {
        if array_ty.is_array().unwrap_or(false) {
            self.element_types.insert(array_ty.element_type());
            self.set_require_infer();
            Ok(())
        } else {
            Err(TypeError::RequireArray(array_ty.clone()))
        }
    }

    fn push_non_array_element(&mut self, ty: &TypeReference) -> Result<(), TypeError> {
        if ty.is_array().unwrap_or(false) {
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
    ) -> Result<(), TypeError> {
        if param_type.is_any_element() {
            self.push_element(arg_type);
        } else if param_type.is_any_array() {
            self.push_array(arg_type)?;
        } else if param_type.is_any_non_array() {
            self.push_non_array_element(arg_type)?;
        } else if param_type.is_any_range() {
            self.push_range_element(arg_type)?;
        } else if param_type.is_any_multi_range() {
            self.push_multi_range_element(arg_type)?;
        }

        Ok(())
    }

    /// get inference result
    pub fn get_inference_result(&self) -> Result<TypeReference, TypeError> {
        if self.element_types.is_empty() {
            return Err(TypeError::SimpleInferNoArgs());
        }
        if self.element_types.len() != 1 {
            return Err(TypeError::SimpleInferAmbigous(
                self.element_types.iter().cloned().collect(),
            ));
        }
        Ok(self.element_types.iter().next().unwrap().clone())
    }

    pub fn set_require_infer(&mut self) {
        self.require_infer = true;
    }

    pub fn is_require_infer(&self) -> bool {
        self.require_infer
    }

    pub fn collect<TParam>(
        &mut self,
        param: &TParam,
        bind: &ParameterBindResult,
    ) -> Result<(), TypeError>
    where
        TParam: FunctionParameterTrait + Sized,
    {
        if let Some(param_type) = param.param_type()
            && param_type.is_simple_infer_arg()
            && param.is_input_param()
        {
            for arg_type in bind.get_arg_types() {
                self.push_input_param(param_type, &arg_type)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_simple_infer_context {
    use testresult::TestResult;

    use crate::sem::{FullName, TypeReference, type_system::TypeError};

    use super::SimpleInferContext;

    #[test]
    fn test_simple_infer_minimun() -> TestResult {
        let mut context = SimpleInferContext::default();
        let param_type = TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int4"));
        let arg_type = TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int4"));

        context.push_input_param(&param_type, &arg_type)?;
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
        context.push_input_param(&param_type, &arg_type)?;

        assert!(context.is_require_infer());
        assert!(context.element_types.contains(&arg_type));

        let param_type =
            TypeReference::from_full_name(FullName::with_schema("pg_catalog", "anyarray"));
        assert!(param_type.is_simple_infer_arg());
        assert!(param_type.is_any_array());
        let arg_type =
            TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", "int4"), true);

        context.push_input_param(&param_type, &arg_type)?;
        assert!(context.is_require_infer());
        assert_eq!(context.element_types.iter().count(), 1);
        assert!(context.get_inference_result().is_ok());

        let param_type =
            TypeReference::from_full_name(FullName::with_schema("pg_catalog", "anyelement"));
        assert!(param_type.is_simple_infer_arg());
        assert!(param_type.is_any_element());
        let arg_type = TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int8"));
        context.push_input_param(&param_type, &arg_type)?;
        assert_eq!(context.element_types.iter().count(), 2);
        assert!(matches!(
            context.get_inference_result(),
            Err(TypeError::SimpleInferAmbigous(_))
        ));

        Ok(())
    }
}

#[derive(Default)]
pub struct CommonInferContext {
    require_infer: bool,
    element_types: HashSet<TypeReference>,
}

impl CommonInferContext {
    pub fn push_compat_element(&mut self, ty: &TypeReference) {
        self.element_types.insert(ty.clone());
    }

    pub fn push_compat_array(&mut self, array_ty: &TypeReference) -> Result<(), TypeError> {
        if array_ty.is_array().unwrap_or(false) {
            self.element_types.insert(array_ty.element_type());
            Ok(())
        } else {
            Err(TypeError::RequireArray(array_ty.clone()))
        }
    }

    pub fn push_compat_non_array(&mut self, ty: &TypeReference) -> Result<(), TypeError> {
        if ty.is_array().unwrap_or(false) {
            Err(TypeError::RequireNonArray(ty.clone()))
        } else {
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
    ) -> Result<(), TypeError> {
        if param_type.is_any_compatible() {
            self.push_compat_element(arg_type);
        } else if param_type.is_any_compatible_array() {
            self.push_compat_array(arg_type)?;
        } else if param_type.is_any_compatible_non_array() {
            self.push_compat_non_array(arg_type)?;
        } else if param_type.is_any_compatible_range() {
            self.push_compat_range_element(arg_type)?;
        } else if param_type.is_any_compatible_multi_range() {
            self.push_compat_multi_range_element(arg_type)?;
        }

        Ok(())
    }

    fn common_types(castables: &IVec<HashSet<TypeReference>>) -> HashSet<TypeReference> {
        let mut iter = castables.iter();
        let Some(first) = iter.next() else {
            return HashSet::new();
        };
        let mut common = first.clone();
        for s in iter {
            common.retain(|t| s.contains(t));
        }
        common
    }

    /// gets common infer result
    pub fn get_inference_result<TParseContext>(
        &self,
        context: &TParseContext,
    ) -> Result<TypeReference, TypeError>
    where
        TParseContext: ParseContext,
    {
        let mut castables: Vec<HashSet<TypeReference>> = Vec::new();

        // enumerate cast definitions
        for source_type in &self.element_types {
            castables.push(context.get_implicit_castable(source_type));
        }

        let candiates = Self::common_types(&castables.into());

        if candiates.is_empty() {
            TypeError::common_infer_no_implict_castable()?;
        }
        if candiates.len() == 1 {
            return Ok(candiates.iter().next().unwrap().clone());
        }

        let mut scoreboard = Vec::new();
        for candiate in candiates {
            let mut score = 0i32;
            for source_type in &self.element_types {
                let Some(cast) = context.get_implicit_cast(source_type, &candiate) else {
                    TypeError::unexpected_cast_not_found()?;
                    TypeError::never();
                };

                score += cast.context().overload_priority_score();
            }
            scoreboard.push((score, candiate));
        }
        let max_score = scoreboard.iter().map(|c| c.0).max().unwrap();
        let results: Vec<_> = scoreboard.iter().filter(|v| v.0 == max_score).collect();
        if results.len() != 1 {
            TypeError::common_infer_ambigous()?;
        }
        Ok(results[0].1.clone())
    }

    pub fn set_require_infer(&mut self) {
        self.require_infer = true;
    }

    pub fn is_require_infer(&self) -> bool {
        self.require_infer
    }

    pub fn collect<TParam>(
        &mut self,
        param: &TParam,
        bind: &ParameterBindResult,
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
                    self.push_input_param(param_type, &arg_type)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_common_infer_context {
    use std::collections::HashSet;

    use testresult::TestResult;

    use crate::sem::{
        CastContext, CastDefinition, FullName, TypeReference, type_system::TypeError,
    };
    use crate::test_helpers::TestParseContext;

    use super::CommonInferContext;

    #[test]
    fn push_input_param_requires_array_argument() -> TestResult {
        let mut context = CommonInferContext::default();
        let param_type = TypeReference::from_full_name(FullName::with_schema(
            "pg_catalog",
            "anycompatiblearray",
        ));
        let arg_type = TypeReference::from_master_type_name("int4");

        let result = context.push_input_param(&param_type, &arg_type);

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

        let result = context.push_input_param(&param_type, &arg_type);

        assert!(matches!(result, Err(TypeError::RequireNonArray(_))));
        assert!(context.element_types.is_empty());
        Ok(())
    }

    #[test]
    fn picks_candidate_with_highest_score() -> TestResult {
        let mut context = CommonInferContext::default();
        let int4 = TypeReference::from_master_type_name("int4");
        let int8 = TypeReference::from_master_type_name("int8");
        context.push_compat_element(&int4);
        context.push_compat_element(&int8);

        let mut parse_context = TestParseContext::default();
        let mut int4_castables = HashSet::new();
        int4_castables.insert(int4.clone());
        int4_castables.insert(int8.clone());
        parse_context.set_get_implicit_castable_result(&int4, &int4_castables);
        let mut int8_castables = HashSet::new();
        int8_castables.insert(int4.clone());
        int8_castables.insert(int8.clone());
        parse_context.set_get_implicit_castable_result(&int8, &int8_castables);

        parse_context.set_get_implicit_cast_result(
            &int4,
            &int4,
            Some(CastDefinition::new(CastContext::NoConversion)),
        );
        parse_context.set_get_implicit_cast_result(
            &int8,
            &int4,
            Some(CastDefinition::new(CastContext::Implicit)),
        );
        parse_context.set_get_implicit_cast_result(
            &int4,
            &int8,
            Some(CastDefinition::new(CastContext::Implicit)),
        );
        parse_context.set_get_implicit_cast_result(
            &int8,
            &int8,
            Some(CastDefinition::new(CastContext::ImplicitPrefeerd)),
        );

        let result = context.get_inference_result(&parse_context)?;

        assert_eq!(result, int4);
        Ok(())
    }

    #[test]
    fn reports_error_when_no_implicit_castable() -> TestResult {
        let mut context = CommonInferContext::default();
        let int4 = TypeReference::from_master_type_name("int4");
        context.push_compat_element(&int4);
        let parse_context = TestParseContext::default();

        assert!(matches!(
            context.get_inference_result(&parse_context),
            Err(TypeError::CommonInferNoImplicitCastable())
        ));
        Ok(())
    }
}
