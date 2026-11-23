use crate::{names::FullName, sem::TextSearchTemplateDefinition};

/// Reads from the effective text search template catalog.
pub trait TextSearchTemplateInfoRead {
    /// Look up an text search template by its qualified name.
    fn text_search_template(&self, _name: &FullName) -> Option<&TextSearchTemplateDefinition> {
        None
    }
}

/// Writes to a mutable schema layer.
pub trait TextSearchTemplateInfoWrite: Sized {
    /// Publish a completed definition whose name was checked during analysis.
    fn register_text_search_template(self, definition: &TextSearchTemplateDefinition) -> Self;
}
