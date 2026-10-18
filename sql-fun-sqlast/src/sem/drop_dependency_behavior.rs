/// Requested dependency behavior for a resolved DROP statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropDependencyBehavior {
    /// Remove dependent objects together with the explicitly selected object.
    Cascade,
    /// Reject removal if another object depends on the selected object.
    Restrict,
}
impl DropDependencyBehavior {
    /// CASCADE dependency behavior.
    pub const fn cascade() -> Self {
        Self::Cascade
    }
    /// RESTRICT dependency behavior, including PostgreSQL's omitted clause.
    pub const fn restrict() -> Self {
        Self::Restrict
    }
    /// Whether dependent objects are included in the requested removal.
    pub const fn is_cascade(self) -> bool {
        matches!(self, Self::Cascade)
    }
}
