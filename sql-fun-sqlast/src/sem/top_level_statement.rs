use crate::sem::{AlterOwner, SemAst};

impl SemAst {
    /// Retain an analyzed ownership change.
    pub fn alter_owner(alteration: &AlterOwner) -> Self {
        Self::AlterOwner(alteration.clone())
    }
    /// Ownership semantics for this statement, when applicable.
    pub const fn ownership_change(&self) -> Option<&AlterOwner> {
        if let Self::AlterOwner(alteration) = self {
            Some(alteration)
        } else {
            None
        }
    }
}
