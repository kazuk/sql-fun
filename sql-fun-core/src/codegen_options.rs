/// Setter ownership contract used by generated row builders.
#[derive(Debug, Clone, Copy, serde::Deserialize, strum::EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum BuilderSetter {
    /// Setters mutate the existing builder.
    Mutable,
    /// Setters consume the builder and return its next state.
    Consuming,
}

impl BuilderSetter {
    /// Select mutable setters.
    #[must_use]
    pub const fn mutable() -> Self {
        Self::Mutable
    }

    /// Select consuming setters.
    #[must_use]
    pub const fn consuming() -> Self {
        Self::Consuming
    }

    /// Whether setters mutate the existing builder.
    #[must_use]
    pub const fn is_mutable(self) -> bool {
        matches!(self, Self::Mutable)
    }
}

/// Independent options controlling SQL function code generation.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct CodegenOptions {
    builder_setter: BuilderSetter,
    builder_build_fallible: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            builder_setter: BuilderSetter::mutable(),
            builder_build_fallible: true,
        }
    }
}

impl CodegenOptions {
    /// Setter ownership contract.
    #[must_use]
    pub const fn builder_setter(&self) -> BuilderSetter {
        self.builder_setter
    }

    /// Whether building a row returns a `Result`.
    #[must_use]
    pub const fn builder_build_fallible(&self) -> bool {
        self.builder_build_fallible
    }

    /// Override the setter contract without changing build fallibility.
    pub const fn set_builder_setter(&mut self, value: BuilderSetter) {
        self.builder_setter = value;
    }

    /// Override build fallibility without changing the setter contract.
    pub const fn set_builder_build_fallible(&mut self, value: bool) {
        self.builder_build_fallible = value;
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen_options::{BuilderSetter, CodegenOptions};

    #[test]
    fn defaults_preserve_existing_contract() {
        let options = CodegenOptions::default();
        assert!(options.builder_setter().is_mutable());
        assert!(options.builder_build_fallible());
    }

    #[test]
    fn setters_are_independent() {
        let mut options = CodegenOptions::default();
        options.set_builder_setter(BuilderSetter::consuming());
        assert!(!options.builder_setter().is_mutable());
        assert!(options.builder_build_fallible());
        options.set_builder_build_fallible(false);
        assert!(!options.builder_setter().is_mutable());
        assert!(!options.builder_build_fallible());
        options.set_builder_setter(BuilderSetter::mutable());
        assert!(options.builder_setter().is_mutable());
        assert!(!options.builder_build_fallible());
    }

    #[test]
    fn serde_and_attribute_parser_agree() {
        for name in ["mutable", "consuming"] {
            let serde: CodegenOptions =
                toml::from_str(&format!("builder_setter = {name:?}")).unwrap();
            let parsed: BuilderSetter = name.parse().unwrap();
            assert_eq!(serde.builder_setter().is_mutable(), parsed.is_mutable());
        }
        assert!("unknown".parse::<BuilderSetter>().is_err());
    }
}
