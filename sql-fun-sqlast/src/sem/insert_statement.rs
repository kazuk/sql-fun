use crate::sem::{AnalysisError, ParseContext, SemAst};

/// semantic AST for `insert` statement
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertStatement {}

/// analyze [`crate::syn::InsertStmt`]
pub fn analyze_insert_statement<TParseContext>(
    _context: TParseContext,
    _parent_schema: &Option<String>,
    _syn: crate::syn::InsertStmt,
) -> Result<(SemAst, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    todo!()
}
