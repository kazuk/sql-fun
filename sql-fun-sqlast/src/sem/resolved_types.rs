//! Owned result metadata that can outlive the solver and enter shared catalogs.

use crate::sem::{InputTypeConstraint, TypeReference};

/// Completed expression type information, independent of an inference graph.
///
/// An absent finite set denotes an unresolved type, an empty set denotes an
/// impossible result, and multiple entries retain legitimate result alternatives.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ResolvedTypes(Option<Vec<TypeReference>>);

impl ResolvedTypes {
    /// An unresolved result type.
    pub const fn any() -> Self {
        Self(None)
    }
    pub(crate) fn types(types: impl IntoIterator<Item = TypeReference>) -> Self {
        let mut types: Vec<_> = types.into_iter().collect();
        types.sort_by_cached_key(|ty| format!("{ty:?}"));
        types.dedup();
        Self(Some(types))
    }
    pub(crate) fn exact(ty: &TypeReference) -> Self {
        Self(Some(vec![ty.clone()]))
    }
    /// Whether the result type remains unrestricted.
    pub const fn is_any(&self) -> bool {
        self.0.is_none()
    }
    /// Completed finite result alternatives, or `None` while unresolved.
    pub fn admissible_types(&self) -> Option<&[TypeReference]> {
        self.0.as_deref()
    }
    /// Whether this type is among the retained result alternatives.
    pub fn allows(&self, ty: &TypeReference) -> bool {
        self.0.as_ref().is_none_or(|types| types.contains(ty))
    }
    /// The result type when exactly one alternative remains.
    pub fn single_type(&self) -> Option<TypeReference> {
        match self.0.as_deref() {
            Some([ty]) => Some(ty.clone()),
            _ => None,
        }
    }
    pub(crate) fn constraint(&self) -> InputTypeConstraint {
        match &self.0 {
            Some(types) => InputTypeConstraint::types(types.iter().cloned()),
            None => InputTypeConstraint::any(),
        }
    }
}

impl From<&InputTypeConstraint> for ResolvedTypes {
    fn from(value: &InputTypeConstraint) -> Self {
        Self(
            value
                .admissible_types()
                .map(|types| types.iter().map(|ty| ty.as_ref().clone()).collect()),
        )
    }
}
impl From<InputTypeConstraint> for ResolvedTypes {
    fn from(value: InputTypeConstraint) -> Self {
        Self::from(&value)
    }
}

impl From<Option<TypeReference>> for ResolvedTypes {
    fn from(value: Option<TypeReference>) -> Self {
        Self(value.map(|ty| vec![ty]))
    }
}
