use turso_parser::ast::QualifiedName as QualifiedNameIn;

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
    pub fn db_name(&self) -> Option<&Name> {
        self.db_name.as_ref()
    }

    /// local name
    pub fn name(&self) -> &Name {
        &self.name
    }
}

use turso_parser::ast::Name as NameIn;

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
        if value.quoted() {
            // HACK: as_ident formats value with quote char, take first char then quote value
            let ident = value.as_ident();
            let quote = ident.chars().next();
            Self {
                value: String::from(value.as_str()),
                quote,
            }
        } else {
            Self {
                value: String::from(value.as_str()),
                quote: None,
            }
        }
    }
}

impl Name {
    /// name text value
    pub fn value(&self) -> &str {
        self.value.as_str()
    }
}
