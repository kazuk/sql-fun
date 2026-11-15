use crate::identifier::CanonicalIdentifier;

/// key of `DefElem`
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct DefElemKey {
    namespace: CanonicalIdentifier,
    name: CanonicalIdentifier,
}

impl DefElemKey {
    fn new(namespace: &str, name: &str) -> Self {
        Self {
            namespace: CanonicalIdentifier::from(namespace),
            name: CanonicalIdentifier::from(name),
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
        if !self.namespace.as_str().is_empty() {
            write!(f, "{}.", self.namespace)?;
        }
        write!(f, "{}", self.name)
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
