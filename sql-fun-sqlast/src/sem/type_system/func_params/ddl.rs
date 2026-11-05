//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::ddl::{FunctionParameter as AstFunctionParameter, ParameterMode as AstParameterMode},
    context::ParseContext,
    names::FullName,
    sem::{AnalysisError, FunctionParam, ScalarAnalysisContext, SemScalarExpr, TypeReference},
};

impl FunctionParam {
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &AstFunctionParameter,
        mut context: TContext,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, &mut context);
        let parameter = context.finish_analysis(result)?;
        Ok((parameter, context))
    }

    pub(crate) fn from_ast(
        ast: &AstFunctionParameter,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = Some(ast.name.clone());
        let type_name = FullName::from_ast_name(&ast.type_name.name)?;
        let param_type = context.resolve_type_name(&type_name).map(|name| {
            TypeReference::concrete_type_ref(&name, !ast.type_name.array_bounds.is_empty())
        });
        let default = ast
            .default
            .as_ref()
            .map(|expr| {
                SemScalarExpr::from_ast_in(expr, context, &ScalarAnalysisContext::default())
            })
            .transpose()?;
        Ok(Self::from_ast_mode(&ast.mode, &name, &param_type, &default))
    }

    pub(crate) fn from_ast_mode(
        ast: &AstParameterMode,
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default: &Option<SemScalarExpr>,
    ) -> Self {
        match ast {
            AstParameterMode::In => Self::new_input_param(name, param_type, default),
            AstParameterMode::Out => Self::new_output_param(name, param_type),
            AstParameterMode::InOut => Self::new_in_out_param(name, param_type, default),
            AstParameterMode::Variadic => Self::new_variadig_param(name, param_type),
            AstParameterMode::Table => Self::new_table_param(name, param_type),
        }
    }
}
