//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::Extension as AstExtension,
    context::ParseContext,
    sem::{AnalysisError, CreateExtension, SemAst},
};

impl CreateExtension {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstExtension,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let extension = Self::from_name(&ast.name).with_if_not_exists(ast.if_not_exists);
        let context = context.apply_create_extension(&extension)?;
        Ok(AstAndContextPair::new(
            SemAst::from_create_extension(&extension),
            context,
        ))
    }
}
