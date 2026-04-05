use crate::sem::AstAndContextPair;

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::AlterOwnerStmt`]
#[derive(Debug, Clone)]
pub struct AlterOwner {}

impl AlterOwner {
    /// analyze [`crate::syn::AlterOwnerStmt`]
    pub fn analyze_alter_owner<TParseContext>(
        context: TParseContext,
        _syn: crate::syn::AlterOwnerStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Ok(AstAndContextPair::new(SemAst::AlterOwner(Self {}), context))
    }
}

/// analyze [`crate::syn::AlterOwnerStmt`]
#[deprecated = "use AlterOwner::analyze_alter_owner"]
#[expect(dead_code)]
pub fn analyze_alter_owner<TParseContext>(
    context: TParseContext,
    syn: crate::syn::AlterOwnerStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    AlterOwner::analyze_alter_owner(context, syn)
}
