use crate::{
    sem::{
        AnalysisError, AnalysisProblem, AstAndContextPair, ColumnCollection, ColumnDefinition,
        DefElemKey, FullName, FunctionOverloadCollection, FunctionParam, OverloadVariant,
        ParseContext, SemAst, type_system::ArgumentBindingCollection,
    },
    syn::{ListOpt, Opt},
};

/// analyzed [crate::syn::CreateFunctionStmt]
#[derive(Debug, Clone)]
pub struct CreateFunction {
    name: FullName,
    overloads: FunctionOverloadCollection,
    returning_columdef: Option<ColumnCollection>,
}

impl CreateFunction {
    /// create new instance
    pub fn new(
        name: &FullName,
        overloads: &FunctionOverloadCollection,
        returning_columns: &Option<ColumnCollection>,
    ) -> Self {
        Self {
            name: name.clone(),
            overloads: overloads.clone(),
            returning_columdef: returning_columns.clone(),
        }
    }

    /// list function overload (Using in builtin / extensions only)
    pub fn overloads(&self) -> &FunctionOverloadCollection {
        &self.overloads
    }

    /// resolve overload with funtion argument types
    pub fn resolve_overload<TParseContext>(
        &self,
        arg_types: &ArgumentBindingCollection,
        context: &mut TParseContext,
    ) -> Option<OverloadVariant>
    where
        TParseContext: ParseContext,
    {
        if self.overloads.len() == 1 {
            self.overloads.get_at(0).cloned()
        } else {
            self.overloads.resolve_overload(context, arg_types)
        }
    }

    /// return trure if function is returns set
    pub fn is_table_function(&self) -> bool {
        self.returning_columdef.is_some()
    }
}

impl CreateFunction {
    /// get function name
    pub fn name(&self) -> &FullName {
        &self.name
    }
}

/// analyze [crate::syn::CreateFunctionStmt]
pub fn analyze_create_function<TParseContext>(
    mut context: TParseContext,
    _parent_schema: &Option<String>,
    syn: crate::syn::CreateFunctionStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let name = FullName::try_from(syn.get_funcname())?;
    let Some(params) = syn.get_parameters().map(|a| a.as_function_parameter()) else {
        AnalysisError::raise_unexpected_none("create_function_stmt.parameters")?
    };

    let Some(ret_type) = syn.get_return_type().as_inner() else {
        AnalysisError::raise_unexpected_none("create_function_stmt.retrun_type")?
    };

    let returning_col_def = if ret_type.get_setof() {
        let ret_type = FullName::try_from(ret_type.clone())?;
        if ret_type.name().as_str() == "record" {
            FunctionParam::collect_output_params_as_columns(&params)?
        } else if let Some(result_set_type) = context.get_type(&ret_type) {
            if result_set_type.is_composit_type() {
                Some(result_set_type.get_composit_type_columns())
            } else if let Some(result_type) = result_set_type.type_reference() {
                // returning record type
                let columns = vec![ColumnDefinition::new(&None, Some(result_type), None)];
                Some(ColumnCollection::new(columns))
            } else {
                // returns dynamic type
                None
            }
        } else {
            context.report_problem(AnalysisProblem::function_result_set_type_not_found(
                &ret_type,
            ));
            Some(ColumnCollection::new(Vec::new()))
        }
    } else {
        None
    };

    let (def_elem_list, new_context) = super::analyze_def_elem_list(context, syn.get_options())?;
    context = new_context;
    let is_strict = def_elem_list.contains(&DefElemKey::strict());
    let mut func_params = Vec::new();
    for param in params {
        let (function_param, new_context) = FunctionParam::analyze(context, param)?;
        context = new_context;
        func_params.push(function_param);
    }

    let ret_type = FullName::try_from(ret_type)?;
    let ret_type = context.get_type(&ret_type);
    let overload_variant = OverloadVariant::new(
        &ret_type.and_then(|v| v.type_reference()).cloned(),
        &func_params,
        is_strict,
        false,
    );

    let func = CreateFunction::new(
        &name,
        &FunctionOverloadCollection::new(&[overload_variant]),
        &returning_col_def,
    );

    let result_context = context.apply_create_function(&func)?;
    Ok(AstAndContextPair::new(
        SemAst::CreateFunction(func),
        result_context,
    ))
}
