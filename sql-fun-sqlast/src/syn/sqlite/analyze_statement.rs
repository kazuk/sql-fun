use crate::syn::sqlite::QualifiedName;

/// analyze statement
#[derive(derive_getters::Getters, Debug)]
pub struct AnalyzeStatement {
    name: Option<QualifiedName>,
}

impl AnalyzeStatement {
    /// create instance
    pub fn new(name: Option<QualifiedName>) -> Self {
        Self { name }
    }
}
