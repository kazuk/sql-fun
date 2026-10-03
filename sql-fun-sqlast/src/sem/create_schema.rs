use sql_fun_core::IVec;

use crate::{sem::AstAndContextPair, sem::unresolved::TryFromWithTokens, syn::ScanToken};

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateSchemaStmt`]
#[derive(Debug, Clone)]
pub struct CreateSchema {
    name: String,
    #[expect(dead_code)]
    elements: Vec<SemAst>,
}

impl CreateSchema {
    pub(crate) fn new(name: &str, elements: &[SemAst]) -> Self {
        Self {
            name: name.to_owned(),
            elements: elements.to_vec(),
        }
    }
    /// new instance
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// analyze [`crate::syn::CreateSchemaStmt`]
    pub fn analyze_create_schema<TParseContext>(
        context: TParseContext,
        syn: crate::syn::CreateSchemaStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::sem::unresolved::Schema::try_from_with_tokens(&syn, tokens)?
            .resolve_and_apply(context, crate::sem::unresolved::Dialect::postgres())
    }
}
