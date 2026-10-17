use crate::sem::FullName;

/// Completed catalog decision about deleting from an ordinary view.
/// DELETE eligibility is independent of INSERT and UPDATE eligibility.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewDeleteSupport {
    deletable: bool,
    returning: bool,
}
impl ViewDeleteSupport {
    /// The view cannot accept DELETE statements.
    pub const fn read_only() -> Self {
        Self {
            deletable: false,
            returning: false,
        }
    }
    /// The view accepts DELETE, with independently established RETURNING support.
    /// The provider accounts for automatic deletion, INSTEAD OF DELETE triggers,
    /// and applicable INSTEAD rules for the configured database version.
    pub const fn deletable(returning: bool) -> Self {
        Self {
            deletable: true,
            returning,
        }
    }
    /// Whether deleting rows from this view is supported.
    pub const fn is_deletable(&self) -> bool {
        self.deletable
    }
    /// Whether DELETE RETURNING is supported.
    pub const fn supports_returning(&self) -> bool {
        self.returning
    }
}

/// DELETE capability service for an already bound, qualified ordinary view.
pub trait ViewDeleteInfoRead {
    /// Return a completed decision without executing trigger or rule bodies.
    /// Catalogs without the required metadata must leave this explicitly
    /// unimplemented, rather than inventing success or a read-only decision.
    fn view_delete_support(&self, view: &FullName) -> ViewDeleteSupport;
}
