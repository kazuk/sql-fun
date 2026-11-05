//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::VariableSet as AstVariableSet,
    context::ParseContext,
    sem::{AnalysisError, ScalarAnalysisContext, SemAst, SemScalarExpr, VariableSet},
};

impl VariableSet {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstVariableSet,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let [argument] = ast.arguments.as_slice() else {
            return AnalysisError::raise_unexpected_input("VariableSetStmt.args len!=1");
        };
        let outcome =
            SemScalarExpr::from_ast(argument, &mut context, &ScalarAnalysisContext::default());
        let value = outcome?;
        let variable_set = Self::new(&ast.name, &value);
        let context = context.apply_variable_set(&variable_set)?;
        Ok(AstAndContextPair::new(
            SemAst::from_variable_set(&variable_set),
            context,
        ))
    }
}
