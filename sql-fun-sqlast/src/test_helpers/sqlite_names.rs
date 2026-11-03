#[cfg(feature = "sqlite")]
use crate::syn::sqlite::Name;

pub(crate) fn assert_bracket_name(name: &Name, expected: &str) {
    assert_eq!(name.value(), expected);
    assert_eq!(*name.quote(), Some('['));
}
