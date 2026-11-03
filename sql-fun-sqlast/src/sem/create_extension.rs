use crate::{sem::AstAndContextPair, syn::CreateExtensionStmt};

use crate::sem::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateExtensionStmt`]
#[derive(Debug, Clone)]
pub struct CreateExtension {
    if_not_exists: bool,

    name: String,
}

impl CreateExtension {
    pub(crate) const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    pub(crate) const fn with_if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }

    pub(crate) fn from_name(name: &str) -> Self {
        Self {
            if_not_exists: false,
            name: name.to_owned(),
        }
    }

    /// get extension name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// analyze [`crate::syn::CreateExtensionStmt`]
    pub fn analyze_create_extension<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: CreateExtensionStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let extension =
            Self::from_name(&syn.get_extname()).with_if_not_exists(syn.get_if_not_exists());
        let result_context = context.apply_create_extension(&extension)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateExtension(extension),
            result_context,
        ))
    }
}
