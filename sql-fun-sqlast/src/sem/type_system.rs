mod call_arguments;
mod err;
mod func_params;
mod function_overload_collection;
mod infer;

pub use self::{
    err::TypeError,
    func_params::FunctionParam,
    function_overload_collection::FunctionOverloadCollection,
    infer::{CommonInferContext, SimpleInferContext, SimpleInferResult},
};

use crate::{
    names::TableName,
    sem::{
        pg_type::TypeReference as PgTypeTypeReference,
        scalar_expr::SemScalarExpr as ScalarExprSemScalarExpr,
    },
};

use std::collections::VecDeque;

use crate::{
    context::ParseContext,
    names::ColumnName,
    sem::{
        ColumnCollection, ColumnDefinition, CreateCompositeType, SemScalarExpr, TypeDefinition,
        TypeReference, create_comp_type::CompositeFieldCollection,
    },
};

use self::func_params::ParameterBindResult;

#[derive(Clone, Debug, Default)]
pub struct ArgumentBindingCollection {
    types: VecDeque<Option<TypeReference>>,
    names: VecDeque<Option<String>>,
    explicit_variadic: bool,
}

impl ArgumentBindingCollection {
    pub(crate) fn types(&self) -> impl Iterator<Item = Option<&TypeReference>> {
        self.types.iter().map(Option::as_ref)
    }

    pub fn from_expr_list(expr_list: &[SemScalarExpr]) -> Self {
        Self {
            types: expr_list
                .iter()
                .map(ScalarExprSemScalarExpr::get_type)
                .collect(),
            names: expr_list
                .iter()
                .map(|expression| {
                    expression
                        .as_named_argument()
                        .map(|named| named.name().to_owned())
                })
                .collect(),
            explicit_variadic: false,
        }
    }

    #[allow(clippy::option_option)]
    pub fn pop_front(&mut self) -> Option<Option<TypeReference>> {
        let ty = self.types.pop_front();
        if ty.is_some() {
            self.names.pop_front();
        }
        ty
    }

    pub fn push(&mut self, arg_type: Option<TypeReference>) {
        self.types.push_back(arg_type);
        self.names.push_back(None);
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
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(ty) = &mut self.ret_type {
            ty.rename_relation(old, new);
        }
        for param in &mut self.params {
            param.rename_relation(old, new);
        }
    }

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
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &SimpleInferResult) {
        if let Some(t) = self.ret_type.as_mut() {
            infer_result_type.apply(t);
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

    pub(crate) fn table_output_depends_on_table(&self, context: &impl ParseContext) -> bool {
        if !self.returning_table().is_empty() {
            return false;
        }
        let Some(ty) = self.ret_type() else {
            return false;
        };
        let Some(definition) = ty.definition_from_catalog(context) else {
            return false;
        };
        definition.row_table_name(context).is_some()
    }

    pub(crate) fn has_scalar_table_output(&self, context: &impl ParseContext) -> bool {
        self.returning_table().is_empty()
            && self
                .ret_type()
                .as_ref()
                .is_some_and(|ty| !ty.is_record() && ty.row_columns(context).is_none())
    }

    pub(crate) fn table_function_output_columns(
        &self,
        context: &impl ParseContext,
        scalar_name: &ColumnName,
    ) -> ColumnCollection {
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
        ColumnCollection::scalar_function_output(scalar_name, ty)
    }
}

impl OverloadVariant {
    pub(crate) fn requires_record_definition(&self) -> bool {
        self.returns_record() && self.returning_table().is_empty()
    }

    fn returns_record(&self) -> bool {
        self.ret_type()
            .as_ref()
            .is_some_and(PgTypeTypeReference::is_record)
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
}

#[cfg(test)]
mod test_output_return_shape {
    use crate::{
        names::ColumnName,
        sem::{FunctionParam, OverloadVariant, TypeReference},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    fn ordered_record_fields_match_table_output(#[values(false, true)] table: bool) {
        let first_name = Some("Mixed Name".to_owned());
        let second_name = Some("label".to_owned());
        let first_type = Some(TypeReference::from_master_type_name("int4"));
        let second_type = Some(TypeReference::from_master_type_name("text"));
        let input = FunctionParam::new_input_param(&Some("input".to_owned()), &first_type, &None);
        let outputs = if table {
            vec![
                input,
                FunctionParam::new_table_param(&first_name, &first_type),
                FunctionParam::new_table_param(&second_name, &second_type),
            ]
        } else {
            vec![
                input,
                FunctionParam::new_in_out_param(&first_name, &first_type, &None),
                FunctionParam::new_output_param(&second_name, &second_type),
            ]
        };
        let overload = OverloadVariant::new(
            &Some(TypeReference::from_master_type_name("record")),
            &outputs,
            false,
            false,
        );
        let result = overload.scaler_ret_type().unwrap();
        let columns = result.row_columns(&TestParseContext::default()).unwrap();
        let table_columns = overload.returning_table();
        let expected_names = vec![ColumnName::from("Mixed Name"), ColumnName::from("label")];
        assert_eq!(columns.names(), expected_names);
        assert_eq!(table_columns.names(), expected_names);
        for (index, (name, ty)) in [("Mixed Name", first_type), ("label", second_type)]
            .into_iter()
            .enumerate()
        {
            let field = columns.column_at(index).unwrap();
            assert_eq!(field.name(), &Some(ColumnName::from(name)));
            assert_eq!(table_columns.column_at(index).unwrap().name(), field.name());
            assert_eq!(field.get_type(), ty.as_ref());
            assert_eq!(field.is_not_null(), Some(false));
            assert_eq!(
                table_columns
                    .get_column_def(&ColumnName::from(name))
                    .unwrap()
                    .get_type(),
                field.get_type(),
            );
        }
        assert!(columns.get_column_def(&ColumnName::from("input")).is_none());
    }

    #[test]
    fn single_inout_keeps_scalar_return_type() {
        let ty = Some(TypeReference::from_master_type_name("int4"));
        let overload = OverloadVariant::new(
            &ty,
            &vec![FunctionParam::new_in_out_param(
                &Some("value".to_owned()),
                &ty,
                &None,
            )],
            false,
            false,
        );
        assert_eq!(overload.scaler_ret_type(), ty);
    }
}

impl OverloadVariant {
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
        if arg_types.has_call_notation() {
            return self.bind_supplied(&arg_types);
        }
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
