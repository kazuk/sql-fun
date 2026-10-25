mod err;
mod func_params;
mod function_overload_collection;
mod infer;

use std::collections::VecDeque;

use crate::sem::{
    ColumnCollection, ColumnDefinition, ColumnName, CreateCompositeType, SemScalarExpr,
    TypeDefinition, TypeReference, create_comp_type::CompositeFieldCollection,
};

use self::func_params::ParameterBindResult;
pub use self::{
    err::TypeError,
    func_params::FunctionParam,
    function_overload_collection::FunctionOverloadCollection,
    infer::{CommonInferContext, CommonInferResult, SimpleInferContext, SimpleInferResult},
};

#[derive(Clone, Debug, Default)]
pub struct ArgumentBindingCollection(VecDeque<Option<TypeReference>>);

impl ArgumentBindingCollection {
    pub fn from_expr_list(expr_list: &[SemScalarExpr]) -> Self {
        Self(
            expr_list
                .iter()
                .map(super::scalar_expr::SemScalarExpr::get_type)
                .collect(),
        )
    }

    #[allow(clippy::option_option)]
    pub fn pop_front(&mut self) -> Option<Option<TypeReference>> {
        self.0.pop_front()
    }

    pub fn push(&mut self, arg_type: Option<TypeReference>) {
        self.0.push_back(arg_type);
    }
}

/// function overload variant
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OverloadVariant {
    ret_type: Option<TypeReference>,
    params: Vec<FunctionParam>,
    is_strict: bool,
    returns_not_null: bool,
}

impl OverloadVariant {
    pub(crate) fn is_variadic(&self) -> bool {
        self.params.iter().any(FunctionParam::is_variadic)
    }

    pub(crate) fn call_argument(&self, position: usize) -> Option<&FunctionParam> {
        let mut input_index = 0;
        for parameter in &self.params {
            if !parameter.is_input() {
                continue;
            }
            if input_index == position || parameter.is_variadic() {
                return Some(parameter);
            }
            input_index += 1;
        }
        None
    }

    pub(crate) fn identity_arguments(&self) -> Vec<Option<TypeReference>> {
        self.params
            .iter()
            .filter_map(FunctionParam::identity_argument)
            .collect()
    }

    pub(crate) fn has_input_parameters(&self) -> bool {
        self.params.iter().any(FunctionParam::is_input)
    }
    pub(crate) fn input_bindings(&self) -> ArgumentBindingCollection {
        let mut arguments = ArgumentBindingCollection::default();
        for parameter in &self.params {
            if parameter.is_input() {
                arguments.push(parameter.get_type().cloned());
            }
        }
        arguments
    }

    /// Apply a simple polymorphic substitution.
    ///
    /// # Errors
    /// Returns an error if a required range family or array container is unavailable.
    pub fn apply_simple_infer_result(
        &mut self,
        infer_result_type: &SimpleInferResult,
    ) -> Result<(), self::TypeError> {
        if let Some(t) = self.ret_type.as_mut() {
            infer_result_type.apply(t)?;
        }

        for param in &mut self.params {
            param.apply_simple_infer_result(infer_result_type)?;
        }
        Ok(())
    }

    /// Apply a common polymorphic substitution.
    ///
    /// # Errors
    /// Returns an error if a required range family or array container is unavailable.
    pub fn apply_common_infer_result(
        &mut self,
        infer_result_type: &crate::sem::type_system::CommonInferResult,
    ) -> Result<(), crate::sem::type_system::TypeError> {
        if let Some(t) = self.ret_type.as_mut() {
            infer_result_type.apply(t)?;
        }

        for param in &mut self.params {
            param.apply_common_infer_result(infer_result_type)?;
        }
        Ok(())
    }

    /// create a variant
    #[must_use]
    pub fn new(
        ret_type: &Option<TypeReference>,
        params: &Vec<FunctionParam>,
        is_strict: bool,
        returns_not_null: bool,
    ) -> Self {
        Self {
            ret_type: ret_type.clone(),
            params: params.to_owned(),
            is_strict,
            returns_not_null,
        }
    }

    /// type of return value
    #[must_use]
    pub const fn ret_type(&self) -> &Option<TypeReference> {
        &self.ret_type
    }

    /// return value type for scalar context
    #[must_use]
    pub fn scaler_ret_type(&self) -> Option<TypeReference> {
        if self.returns_record() {
            let mut composite_type_fields = CompositeFieldCollection::default();
            for param in &self.params {
                if let Some(field_def) = param.field_definition() {
                    composite_type_fields.push(field_def);
                }
            }
            let composite = CreateCompositeType::from_fields(&composite_type_fields);
            let composite_type_def = TypeDefinition::CompositeType(composite);
            Some(TypeReference::dynamic(&composite_type_def))
        } else {
            self.ret_type.clone()
        }
    }

    pub(crate) fn table_function_output_columns(
        &self,
        context: &impl crate::sem::ParseContext,
        scalar_name: &crate::sem::ColumnName,
    ) -> crate::sem::ColumnCollection {
        let declared = self.returning_table();
        if !declared.is_empty() {
            return declared;
        }
        let Some(ty) = self.ret_type() else {
            return declared;
        };
        if let Some(definition) = ty.definition_from_catalog(context)
            && let Some(columns) = definition.row_columns(context)
        {
            return columns;
        }
        crate::sem::ColumnCollection::scalar_function_output(scalar_name, ty)
    }

    pub(crate) fn requires_record_definition(&self) -> bool {
        self.returns_record() && self.returning_table().is_empty()
    }

    fn returns_record(&self) -> bool {
        self.ret_type()
            .as_ref()
            .is_some_and(super::pg_type::TypeReference::is_record)
    }

    /// get argument by zero based index
    #[must_use]
    pub fn get_arg_def_at(&self, index: usize) -> Option<&FunctionParam> {
        self.params.get(index)
    }

    /// function is strict
    #[must_use]
    pub const fn is_strict(&self) -> bool {
        self.is_strict
    }

    /// function returns not null
    #[must_use]
    pub const fn returns_not_null(&self) -> bool {
        self.returns_not_null
    }

    /// required parameter count
    #[must_use]
    pub const fn required_param_count(&self) -> usize {
        // TODO: support optional / variadic params
        self.params.len()
    }

    /// table function result set definitions
    #[must_use]
    pub fn returning_table(&self) -> ColumnCollection {
        let mut column_collection = ColumnCollection::default();

        for param in &self.params {
            param.append_returning_column(&mut column_collection);
        }
        column_collection
    }

    /// Get column from function result set
    #[must_use]
    pub fn get_result_column_def(&self, column: &ColumnName) -> Option<ColumnDefinition> {
        let retuning_table = self.returning_table();
        retuning_table.get_column_def(column).cloned()
    }

    /// get parameter list
    #[must_use]
    pub const fn params(&self) -> &Vec<FunctionParam> {
        &self.params
    }

    /// bind arguments
    fn bind_args(
        &self,
        mut arg_types: ArgumentBindingCollection,
    ) -> Result<Vec<ParameterBindResult>, TypeError> {
        let mut results = Vec::new();
        for param in &self.params {
            results.push(param.bind_arg(&mut arg_types)?);
        }
        if arg_types.pop_front().is_some() {
            return Err(TypeError::too_many_arguments());
        }
        Ok(results)
    }
}
