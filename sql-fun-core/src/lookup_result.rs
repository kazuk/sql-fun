/// The conclusiveness of a lookup within its searched scope.
///
/// Only [`Self::NotFound`] establishes absence and permits an enclosing-scope
/// lookup. [`Self::Unknown`] must not be treated as absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupResult<T> {
    /// A value has been conclusively resolved.
    Found(T),
    /// Absence has been established in the searched scope.
    NotFound,
    /// Available information cannot establish either a binding or absence.
    Unknown,
}

impl<T> LookupResult<T> {
    /// Record a conclusively resolved value.
    #[must_use]
    pub const fn found(value: T) -> Self {
        Self::Found(value)
    }

    /// Record conclusive absence.
    #[must_use]
    pub const fn not_found() -> Self {
        Self::NotFound
    }

    /// Record an inconclusive lookup.
    #[must_use]
    pub const fn unknown() -> Self {
        Self::Unknown
    }

    /// Whether absence is conclusive and enclosing-scope lookup is permitted.
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
}
