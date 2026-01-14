use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, FromClause, FullName, OverloadVariant, ParseContext,
        SemScalarExpr, TypeReference, WithClause, type_system::ArgumentBindingCollection,
    },
    syn::{ListOpt, ScanToken},
};

use super::{SemScalarExprNode, analyze_scaler_expr};

mod implementation {
    use sql_fun_core::IVec;

    use crate::sem::{FullName, FunctionParam, OverloadVariant, SemScalarExpr};

    /// function call expression
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallExpr {
        func_name: FullName,
        overload: Option<OverloadVariant>,
        args: IVec<FuncCallArgs>,
    }

    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallArgs {
        arg_expr: SemScalarExpr,
        arg_definition: Option<FunctionParam>,
    }

    impl FuncCallExpr {
        /// create a instance
        #[must_use]
        pub fn new(
            func_name: &FullName,
            overload: &Option<OverloadVariant>,
            args: &IVec<FuncCallArgs>,
        ) -> Self {
            Self {
                func_name: func_name.clone(),
                overload: overload.clone(),
                args: args.clone(),
            }
        }

        /// resolved calling overload
        #[must_use]
        pub fn overload(&self) -> &Option<OverloadVariant> {
            &self.overload
        }

        /// argument expression and definitions
        #[must_use]
        pub fn args(&self) -> &IVec<FuncCallArgs> {
            &self.args
        }
    }

    impl FuncCallArgs {
        pub fn new(arg_expr: &SemScalarExpr, arg_definition: &Option<FunctionParam>) -> Self {
            Self {
                arg_expr: arg_expr.clone(),
                arg_definition: arg_definition.clone(),
            }
        }

        pub fn arg_expr(&self) -> &SemScalarExpr {
            &self.arg_expr
        }

        pub fn arg_definition(&self) -> &Option<FunctionParam> {
            &self.arg_definition
        }
    }
}

pub use self::implementation::{FuncCallArgs, FuncCallExpr};

impl FuncCallExpr {
    fn partial_response<TParseContext>(
        context: TParseContext,
        func_name: FullName,
        arg_exprs: Vec<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let args = arg_exprs
            .iter()
            .map(|a| FuncCallArgs::new(a, &None))
            .collect();
        let fc = FuncCallExpr::new(&func_name, &None, &args);
        Ok((SemScalarExpr::FuncCall(fc), context))
    }

    fn cast_operand_expression<TParseContext>(
        context: &mut TParseContext,
        func_name: &FullName,
        arg_exprs: &IVec<SemScalarExpr>,
        overload: &OverloadVariant,
    ) -> Result<IVec<FuncCallArgs>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let mut args = Vec::new();
        for (index, arg) in arg_exprs.iter().enumerate() {
            let mut arg = arg.clone();
            let arg_def = overload.get_arg_def_at(index);
            if let Some(arg_definition) = arg_def
                && let Some(arg_def_type) = arg_definition.get_type()
                && let Some(arg_val_type) = arg.get_type()
            {
                let Some(t) = context.get_type(arg_def_type.full_name()) else {
                    AnalysisError::raise_unexpected_input(&format!(
                        "type {arg_def_type} not found in context"
                    ))?
                };
                let Some(vt) = context.get_type(arg_val_type.full_name()) else {
                    AnalysisError::raise_unexpected_input(&format!(
                        "type {arg_val_type} not found in context"
                    ))?
                };

                if let Some(source_type) = vt.type_reference()
                    && let Some(target_type) = t.type_reference()
                    && let Some(cast) = context.get_implicit_cast(source_type, target_type)
                {
                    if !cast.is_no_coversion() {
                        arg = SemScalarExpr::wrap_implicit_cast(target_type, &arg);
                    }
                } else {
                    context.report_problem(
                        AnalysisProblem::function_arg_implicit_cast_not_found(
                            func_name, index, t, &arg,
                        ),
                    )?;
                }
            }

            args.push(FuncCallArgs::new(&arg, &arg_def.cloned()));
        }
        Ok(args.into())
    }
}

impl SemScalarExprNode for FuncCallExpr {
    fn get_type(&self) -> Option<TypeReference> {
        let Some(overload) = &self.overload() else {
            return None;
        };
        overload.scaler_ret_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        if let Some(overload) = &self.overload() {
            if overload.is_strict() {
                for arg in self.args() {
                    let arg_is_not_null = matches!(arg.arg_expr().is_not_null(), Some(true));

                    if !arg_is_not_null {
                        return Some(false);
                    }
                }
            }
            Some(overload.returns_not_null())
        } else {
            // when overload not resolved, returns None meaning Unknown
            None
        }
    }
}

impl<TParseContext> super::AnalyzeScalarExpr<TParseContext, crate::syn::FuncCall> for FuncCallExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::FuncCall,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let func_name = FullName::try_from(syn.get_funcname())?;

        let Some(args) = syn.get_args().as_inner() else {
            AnalysisError::raise_unexpected_none("funccall.args")?
        };

        let mut arg_exprs = Vec::new();
        for arg in args {
            let (sem_arg, new_context) =
                analyze_scaler_expr(context, with_clause, from_clause, arg, tokens)?;
            context = new_context;
            arg_exprs.push(sem_arg);
        }

        if let Some(overloads) = context.get_function_by_name(&func_name) {
            let arg_types = ArgumentBindingCollection::from_expr_list(&arg_exprs);
            if let Some(overload) = overloads.resolve_overload(&mut context, &arg_types) {
                let args = Self::cast_operand_expression(
                    &mut context,
                    &func_name,
                    &arg_exprs.into(),
                    &overload,
                )?;
                let fc = FuncCallExpr::new(&func_name, &Some(overload.clone()), &args);
                Ok((SemScalarExpr::FuncCall(fc), context))
            } else {
                context.report_problem(AnalysisProblem::function_overload_resolution_failed(
                    &func_name, &arg_types,
                ))?;
                Self::partial_response(context, func_name, arg_exprs)
            }
        } else {
            context.report_problem(AnalysisProblem::function_not_found(&func_name))?;
            Self::partial_response(context, func_name, arg_exprs)
        }
    }
}
