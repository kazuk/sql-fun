use crate::sem::{AnalysisError, ParseContext, SemAst};

/// `update` statement sem AST
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateStatement {}

impl UpdateStatement {
    pub(crate) const fn empty() -> Self {
        Self {}
    }
}

/// analyze `update` statement
///
/// # Errors
///
/// Returns [`AnalysisError`] when semantic analysis fails.
pub fn analyze_update_statement<TParseContext>(
    context: TParseContext,
    _parent_schema: &Option<String>,
    _syn: crate::syn::UpdateStmt,
) -> Result<(SemAst, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    Ok((SemAst::UpdateStatement(UpdateStatement {}), context))
}
