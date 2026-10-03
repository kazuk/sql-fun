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
