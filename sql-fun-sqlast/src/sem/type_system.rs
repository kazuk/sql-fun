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
    infer::{CommonInferContext, SimpleInferContext},
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature_metadata: Option<FunctionSignatureMetadata>,
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

    /// apply simple infer result
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &TypeReference) {
        if let Some(t) = self.ret_type.as_mut() {
            t.apply_simple_infer_result(infer_result_type);
        }

        for param in &mut self.params {
            param.apply_simple_infer_result(infer_result_type);
        }
    }

    /// apply common infer result
    pub fn apply_common_infer_result(&mut self, infer_result_type: &TypeReference) {
        if let Some(t) = self.ret_type.as_mut() {
            t.apply_common_infer_result(infer_result_type);
        }

        for param in &mut self.params {
            param.apply_common_infer_result(infer_result_type);
        }
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
            signature_metadata: None,
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

/// Retained declaration facts, separate from the legacy computed return type.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionSignatureMetadata {
    result: Option<TypeReference>,
    returns_set: bool,
    procedure: bool,
    default_sql: Option<String>,
    default_start: usize,
    parameters: Vec<FunctionParam>,
}

impl FunctionSignatureMetadata {
    /// Preserve declaration facts supplied by the producer, without guessing missing values.
    pub fn new(
        result: &Option<TypeReference>,
        parameters: &[FunctionParam],
        returns_set: bool,
        procedure: bool,
    ) -> Self {
        Self {
            result: result.clone(),
            returns_set,
            procedure,
            default_sql: None,
            default_start: 0,
            parameters: parameters.to_vec(),
        }
    }
    /// Parameters in declaration order, before legacy normalization.
    pub fn parameters(&self) -> &[FunctionParam] {
        &self.parameters
    }
    /// Retain the complete PostgreSQL default-expression list and first defaulted input index.
    pub fn with_default_sql(mut self, sql: &str, start: usize) -> Self {
        self.default_sql = Some(sql.to_owned());
        self.default_start = start;
        self
    }
    /// Original declared result, before legacy output-record synthesis.
    pub const fn result(&self) -> Option<&TypeReference> {
        self.result.as_ref()
    }
    /// Whether the declaration returns a set.
    pub const fn returns_set(&self) -> bool {
        self.returns_set
    }
    /// Whether this is a procedure rather than a function.
    pub const fn is_procedure(&self) -> bool {
        self.procedure
    }
    /// Complete SQL default-expression list, if supplied by the extractor.
    pub fn default_sql(&self) -> Option<&str> {
        self.default_sql.as_deref()
    }
    /// First defaulted input index in the callable input sequence.
    pub const fn default_start(&self) -> usize {
        self.default_start
    }
}

impl OverloadVariant {
    /// Attach complete declaration metadata without changing legacy execution fields.
    pub fn with_signature_metadata(mut self, metadata: &FunctionSignatureMetadata) -> Self {
        self.signature_metadata = Some(metadata.clone());
        self
    }
    /// Declaration metadata, absent for an incomplete/old producer.
    pub const fn signature_metadata(&self) -> Option<&FunctionSignatureMetadata> {
        self.signature_metadata.as_ref()
    }
}
