use crate::sem::{AnalysisError, ParseContext, SemAst};

/// `DELETE` statement
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeleteStatement {}

/// analyze [`crate::syn::DeleteStmt`]
pub fn analyze_delete_statement<TParseContext>(
    _context: TParseContext,
    _parent_schema: &Option<String>,
    _syn: crate::syn::DeleteStmt,
) -> Result<(SemAst, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    todo!()
}
