/// Explicit choices controlling executable SQL rewriting for a datasource.
#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
#[serde(default)]
pub struct RewriteOptions {
    expand_asterisk: Option<bool>,
}

impl RewriteOptions {
    /// Whether table-dependent wildcard projections should be expanded.
    ///
    /// Only `Some(true)` enables expansion; omission and `Some(false)` preserve
    /// the existing executable SQL behavior.
    #[must_use]
    pub const fn expand_asterisk(&self) -> Option<bool> {
        self.expand_asterisk
    }
}

#[cfg(test)]
mod tests {
    use crate::rewrite_options::RewriteOptions;

    #[test]
    fn default_preserves_omitted_choice() {
        assert_eq!(RewriteOptions::default().expand_asterisk(), None);
    }
}
