use crate::{names::FullName, sem::TextSearchDictionaryDefinition};

/// Reads from the effective text search dictionary catalog.
pub trait TextSearchDictionaryInfoRead {
    /// Look up a text search dictionary by its qualified name.
    fn text_search_dictionary(&self, _name: &FullName) -> Option<&TextSearchDictionaryDefinition> {
        None
    }
}

/// Writes to a mutable schema layer.
pub trait TextSearchDictionaryInfoWrite: Sized {
    /// Publish a completed definition whose name was checked during analysis.
    fn register_text_search_dictionary(self, definition: &TextSearchDictionaryDefinition) -> Self;
}
