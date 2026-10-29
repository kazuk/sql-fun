//! Lexical provenance is retained for edits but excluded from semantic equality.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SourceFlag(bool);
impl SourceFlag {
    pub(crate) const fn new(value: bool) -> Self {
        Self(value)
    }
    pub(crate) const fn get(self) -> bool {
        self.0
    }
}
impl PartialEq for SourceFlag {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
impl Eq for SourceFlag {}
impl std::hash::Hash for SourceFlag {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}
