use crate::sem::AstAndContextPair;

use super::{AnalysisError, ParseContext, SemAst};

#[allow(missing_docs)]
#[deprecated]
#[derive(Debug, Clone)]
pub struct AIndirection {}

impl AIndirection {
    #[allow(missing_docs)]
    #[deprecated]
    pub fn analyze_a_indirection<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        _syn: crate::AIndirection,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Ok(AstAndContextPair::new(
            SemAst::AIndirection(AIndirection {}),
            context,
        ))
    }
}

#[allow(missing_docs)]
#[deprecated = "use AIndirection::analyze_a_indirection"]
pub fn analyze_a_indirection<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::AIndirection,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    AIndirection::analyze_a_indirection(context, parent_schema, syn)
}
