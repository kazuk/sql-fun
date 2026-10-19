/// PostgreSQL type classification code, including user-defined ASCII codes.
///
/// This is independent of structural type kind, cast availability, and preferred status.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "char", into = "char")]
pub struct TypeCategory(char);

impl TypeCategory {
    /// The exact PostgreSQL catalog code.
    pub const fn code(self) -> char {
        self.0
    }
}

impl TryFrom<char> for TypeCategory {
    type Error = &'static str;

    fn try_from(code: char) -> Result<Self, Self::Error> {
        if (' '..='~').contains(&code) {
            Ok(Self(code))
        } else {
            Err("type category must be one printable ASCII character")
        }
    }
}

impl From<TypeCategory> for char {
    fn from(category: TypeCategory) -> Self {
        category.code()
    }
}

#[cfg(test)]
mod tests {
    use super::TypeCategory;

    #[rstest::rstest]
    #[case::internal('Z')]
    #[case::unknown('X')]
    #[case::custom('!')]
    #[case::lowercase('q')]
    #[case::space(' ')]
    #[case::last_printable('~')]
    fn round_trips_exact_code(#[case] code: char) {
        let category = TypeCategory::try_from(code).unwrap();
        let json = serde_json::to_string(&category).unwrap();
        assert_eq!(
            serde_json::from_str::<TypeCategory>(&json).unwrap(),
            category
        );
        assert_eq!(category.code(), code);
    }

    #[rstest::rstest]
    #[case::empty(r#""""#)]
    #[case::multiple(r#""NS""#)]
    #[case::non_ascii(r#""é""#)]
    #[case::nul(r#""\u0000""#)]
    #[case::control(r#""\n""#)]
    #[case::del(r#""\u007f""#)]
    fn rejects_invalid_codes(#[case] json: &str) {
        assert!(serde_json::from_str::<TypeCategory>(json).is_err());
    }
}
