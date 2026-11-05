//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::EnumType as AstEnumType,
    context::ParseContext,
    names::FullName,
    sem::{AnalysisError, CreateEnum, SemAst},
};

impl CreateEnum {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstEnumType,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let enum_type = Self::new(&FullName::from_ast_name(&ast.name)?, &ast.values);
        let context = context.apply_create_enum(&enum_type)?;
        Ok(AstAndContextPair::new(
            SemAst::from_create_enum(&enum_type),
            context,
        ))
    }
}
