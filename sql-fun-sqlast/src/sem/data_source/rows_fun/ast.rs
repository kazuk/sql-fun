use crate::{
    ast::Expr,
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, DataSource, DataSourceRowsFromFunc, ScalarAnalysisContext,
        SemScalarExpr, TableFunctionOutput,
    },
};

impl DataSourceRowsFromFunc {
    pub(crate) fn from_ast(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_call(ast, context, scope, None).and_then(|mut source| {
            source.apply_output(context, &TableFunctionOutput::default())?;
            Ok(source)
        });
        context.finish_analysis(result)
    }
}

impl DataSourceRowsFromFunc {
    pub(crate) fn from_ast_call(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        if let Expr::Call(call) = ast
            && SemScalarExpr::ast_is_ordinary(&call.kind)
        {
            return Ok(Self::invocation(&DataSource::from_ast_table(
                call.as_ref(),
                context,
                scope,
                alias,
            )?));
        }
        let value = SemScalarExpr::from_ast_source_expression(ast, context, scope)?;
        let name = if let Some(alias) = alias {
            alias.name().to_string()
        } else {
            TableFunctionOutput::ast_expression_name(ast, &value)?
        };
        Self::expression(&value, &name, context)
    }
}
