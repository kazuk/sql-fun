//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::Comment as AstComment,
    context::ParseContext,
    sem::{AnalysisError, Comment, SemAst},
};

impl Comment {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstComment,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let target = ast
            .canonical_target()
            .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
        let comment = Self::new(&target, ast.object_kind, &ast.text);
        let context = context.apply_comment(&comment)?;
        Ok(AstAndContextPair::new(
            SemAst::from_comment(&comment),
            context,
        ))
    }
}
