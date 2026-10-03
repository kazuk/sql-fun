use crate::sem::AstAndContextPair;

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateExtensionStmt`]
#[derive(Debug, Clone)]
pub struct CreateExtension {
    name: String,
}

impl CreateExtension {
    /// get extension name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// analyze [`crate::syn::CreateExtensionStmt`]
    pub fn analyze_create_extension<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::CreateExtensionStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let name = syn.get_extname();
        let extension = Self { name };
        let result_context = context.apply_create_extension(&extension)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateExtension(extension),
            result_context,
        ))
    }
}
