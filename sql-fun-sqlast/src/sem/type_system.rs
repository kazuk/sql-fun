mod err;
mod func_params;
mod function_overload_collection;
mod infer;

use std::collections::VecDeque;

use crate::sem::{
    ColumnCollection, ColumnDefinition, ColumnName, CreateCompositType, SemScalarExpr,
    TypeDefinition, TypeReference, create_comp_type::CompositeFieldCollection,
};

use self::func_params::ParameterBindResult;
pub use self::{
    err::TypeError,
    func_params::FunctionParam,
    function_overload_collection::FunctionOverloadCollection,
    infer::{CommonInferContext, SimpleInferContext},
};

#[derive(Clone, Debug, Default)]
pub struct ArgumentBindingCollection(VecDeque<Option<TypeReference>>);

impl ArgumentBindingCollection {
    pub fn from_expr_list(expr_list: &[SemScalarExpr]) -> Self {
        Self(VecDeque::from_iter(expr_list.iter().map(|a| a.get_type())))
    }

    pub fn pop_front(&mut self) -> Option<Option<TypeReference>> {
        self.0.pop_front()
    }

    pub fn push(&mut self, arg_type: Option<TypeReference>) {
        self.0.push_back(arg_type)
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
    /// apply simple infer result
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &TypeReference) {
        if let Some(t) = self.ret_type.as_mut() {
            t.apply_simple_infer_result(infer_result_type)
        }

        for param in self.params.iter_mut() {
            param.apply_simple_infer_result(infer_result_type)
        }
    }

    /// apply common infer result
    pub fn apply_common_infer_result(&mut self, infer_result_type: &TypeReference) {
        if let Some(t) = self.ret_type.as_mut() {
            t.apply_common_infer_result(infer_result_type)
        }

        for param in self.params.iter_mut() {
            param.apply_common_infer_result(infer_result_type)
        }
    }

    /// create a variant
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
    pub fn ret_type(&self) -> &Option<TypeReference> {
        &self.ret_type
    }

    /// return value type for scalar context
    pub fn scaler_ret_type(&self) -> Option<TypeReference> {
        if self.returns_record() {
            let mut composite_type_fields = CompositeFieldCollection::default();
            for param in &self.params {
                if let Some(field_def) = param.field_definition() {
                    composite_type_fields.push(field_def)
                }
            }
            let composite = CreateCompositType::from_fields(&composite_type_fields);
            let composite_type_def = TypeDefinition::CompositType(composite);
            Some(TypeReference::dynamic(&composite_type_def))
        } else {
            self.ret_type.clone()
        }
    }

    fn returns_record(&self) -> bool {
        self.ret_type().as_ref().is_some_and(|t| t.is_record())
    }

    /// get argment by zero based index
    pub fn get_arg_def_at(&self, index: usize) -> Option<&FunctionParam> {
        self.params.get(index)
    }

    /// function is strict
    pub fn is_strict(&self) -> bool {
        self.is_strict
    }

    /// function returns not null
    pub fn returns_not_null(&self) -> bool {
        self.returns_not_null
    }

    /// required parameter count
    pub fn required_param_count(&self) -> usize {
        // TODO: support optional / variadic params
        self.params.len()
    }

    /// table fuction result set definitions
    pub fn returning_table(&self) -> ColumnCollection {
        let mut column_collection = ColumnCollection::default();

        for param in &self.params {
            param.append_returning_column(&mut column_collection)
        }
        column_collection
    }

    /// Get column from funtion result set
    pub fn get_result_column_def(&self, column: &ColumnName) -> Option<ColumnDefinition> {
        let retuning_table = self.returning_table();
        retuning_table.get_column_def(column).cloned()
    }

    /// get parameter list
    pub fn params(&self) -> &Vec<FunctionParam> {
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
        Ok(results)
    }
}
