use crate::sem::AstAndContextPair;

use super::{AnalysisError, ParseContext, SemAst};

/// type cast
#[derive(Clone, Debug)]
pub struct TypeCast {}

/// analyze [crate::TypeCast]
#[deprecated]
pub fn analyze_type_cast<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::TypeCast,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    Ok(AstAndContextPair::new(
        SemAst::TypeCast(TypeCast {}),
        context,
    ))
}
