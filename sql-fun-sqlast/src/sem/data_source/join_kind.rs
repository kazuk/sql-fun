use crate::{ast::JoinKind as AstJoinKind, sem::Nullable};

/// table join kind
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum JoinKind {
    /// inner join
    Inner,
    /// left outer join
    Left,
    /// right outer join
    Right,
    /// cross join
    Full,
    /// semi join  (may be not use in SQL syntax)
    Semi,
    /// Right semi join in PostgreSQL internal trees; not raw SQL syntax.
    RightSemi,
    /// anti join  (may be not use in SQL syntax)
    Anti,
    /// right anti join  (may be not use in SQL syntax)
    RightAnti,
    /// unique inner join  (may be not use in SQL syntax)
    UniqueInner,
    /// unique outer join  (may be not use in SQL syntax)
    UniqueOuter,
}

impl JoinKind {
    pub(super) const fn uses_right_join_column(&self) -> bool {
        matches!(self, Self::Right)
    }

    pub(crate) const fn inner() -> Self {
        Self::Inner
    }
    pub(crate) const fn left() -> Self {
        Self::Left
    }
    pub(crate) const fn right() -> Self {
        Self::Right
    }
    pub(crate) const fn full() -> Self {
        Self::Full
    }

    /// get left nullability
    #[must_use]
    pub const fn left_nullable(&self, in_nullable: Nullable) -> Nullable {
        match self {
            Self::Inner => in_nullable,
            Self::Left => in_nullable,
            Self::Right => true,
            Self::Full => true,
            Self::Semi | Self::RightSemi => in_nullable,
            Self::Anti => in_nullable,
            Self::RightAnti => false,
            Self::UniqueInner => in_nullable,
            Self::UniqueOuter => true,
        }
    }

    /// get right nullability
    #[must_use]
    pub const fn right_nullable(&self, in_nullable: Nullable) -> Nullable {
        match self {
            Self::Inner => in_nullable,
            Self::Left => true,
            Self::Right => in_nullable,
            Self::Full => true,
            Self::Semi | Self::RightSemi => in_nullable,
            Self::Anti => in_nullable,
            Self::RightAnti => false,
            Self::UniqueInner => in_nullable,
            Self::UniqueOuter => true,
        }
    }
}

impl JoinKind {
    pub(super) const fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }
}

impl JoinKind {
    pub(crate) const fn from_ast(ast: &AstJoinKind) -> Self {
        match ast {
            AstJoinKind::Inner | AstJoinKind::Cross => Self::inner(),
            AstJoinKind::Left => Self::left(),
            AstJoinKind::Right => Self::right(),
            AstJoinKind::Full => Self::full(),
        }
    }
}
