use ::turso_parser::ast::QualifiedName as QualifiedNameIn;

/// qualified name
#[derive(Clone, Debug, derive_getters::Getters)]
#[cfg_attr(test, derive(derive_builder::Builder))]
pub struct QualifiedName {
    /// db name
    #[getter(skip)]
    #[cfg_attr(test, builder(setter(into, strip_option), default))]
    db_name: Option<Name>,
    /// name
    #[getter(skip)]
    #[cfg_attr(test, builder(setter(into)))]
    name: Name,
    /// alias
    #[cfg_attr(test, builder(setter(into, strip_option), default))]
    alias: Option<Name>,
}

impl From<QualifiedNameIn> for QualifiedName {
    fn from(value: QualifiedNameIn) -> Self {
        Self {
            db_name: value.db_name.map(Name::from),
            name: Name::from(value.name),
            alias: value.alias.map(Name::from),
        }
    }
}

impl QualifiedName {
    /// database name
    pub const fn db_name(&self) -> Option<&Name> {
        self.db_name.as_ref()
    }

    /// local name
    pub const fn name(&self) -> &Name {
        &self.name
    }
}

use ::turso_parser::ast::Name as NameIn;

/// name value
#[derive(Clone, Debug, derive_getters::Getters)]
#[cfg_attr(test, derive(derive_builder::Builder))]
pub struct Name {
    /// quote
    #[cfg_attr(test, builder(setter(into, strip_option), default))]
    quote: Option<char>,
    /// value
    #[getter(skip)]
    #[cfg_attr(test, builder(setter(into)))]
    value: String,
}

impl From<NameIn> for Name {
    fn from(value: NameIn) -> Self {
        let quote = ['\'', '"', '`', '[']
            .into_iter()
            .find(|quote| value.quoted_with(*quote));
        Self {
            value: String::from(value.as_str()),
            quote,
        }
    }
}

impl Name {
    /// name text value
    pub const fn value(&self) -> &str {
        self.value.as_str()
    }
}

#[cfg(test)]
mod test_name_conversion {
    use super::{Name, NameIn};

    #[test]
    fn preserves_quote_kind_even_when_rendering_uses_a_fallback() {
        for (input, expected, quote) in [
            ("plain", "plain", None),
            ("[items]", "items", Some('[')),
            ("\"items\"", "items", Some('"')),
            ("`items`", "items", Some('`')),
            ("'items'", "items", Some('\'')),
            ("\"1item\"", "1item", Some('"')),
        ] {
            let owned = Name::from(NameIn::from_string(input));
            assert_eq!(owned.value(), expected);
            assert_eq!(*owned.quote(), quote);
        }
        let upstream = NameIn::bracketed("contains]bracket".to_owned());
        assert!(upstream.as_ident().starts_with('"'));
        let owned = Name::from(upstream);
        assert_eq!(owned.value(), "contains]bracket");
        assert_eq!(*owned.quote(), Some('['));
        let owned = Name::from(NameIn::exact("1item".to_owned()));
        assert_eq!(owned.value(), "1item");
        assert_eq!(*owned.quote(), None);
    }
}
