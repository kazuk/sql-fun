use crate::{
    ast::Call,
    context::ParseContext,
    names::FullName,
    sem::{AnalysisError, FuncCallExpr, SemScalarExpr},
};

impl FuncCallExpr {
    pub(crate) fn from_ast_supplied_call(
        ast: &Call,
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<Self, AnalysisError> {
        Self::resolve_supplied(
            context,
            &FullName::from_ast_name(&ast.name)?,
            arguments,
            &ast.argument_locations,
            &ast.location,
            ast.variadic,
            ast.over.is_some(),
        )
    }
}
