/// key of `DefElem`
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct DefElemKey {
    namespace: String,
    name: String,
}

impl DefElemKey {
    fn new(namespace: &str, name: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            name: name.to_string(),
        }
    }

    /// strict `DefElem` name
    #[must_use]
    pub fn strict() -> Self {
        Self::new("", "strict")
    }
}

impl std::fmt::Display for DefElemKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.namespace.is_empty() {
            write!(
                f,
                "{}.",
                crate::identifier::CanonicalIdentifier::from(self.namespace.as_str())
            )?;
        }
        write!(
            f,
            "{}",
            crate::identifier::CanonicalIdentifier::from(self.name.as_str())
        )
    }
}

#[cfg(test)]
mod display_tests {
    use crate::sem::def_elem::DefElemKey;

    #[rstest::rstest]
    #[case::local("", "strict", "strict")]
    #[case::qualified("storage", "enabled", "storage.enabled")]
    #[case::escaped("a.b", "a\"b", r#""a.b"."a""b""#)]
    fn presents_logical_option_components(
        #[case] namespace: &str,
        #[case] name: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(DefElemKey::new(namespace, name).to_string(), expected);
    }
}
