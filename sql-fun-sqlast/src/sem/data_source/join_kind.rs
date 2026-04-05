use crate::{
    sem::{AnalysisError, Nullable},
    syn::{JoinType, JoinTypeOpt, Opt},
};

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
    /// anti join  (may be not use in SQL syntax)
    Anti,
    /// right anti join  (may be not use in SQL syntax)
    RightAnti,
    /// unique inner join  (may be not use in SQL syntax)
    UniqueInner,
    /// unique outer join  (may be not use in SQL syntax)
    UniqueOuter,
}

impl TryFrom<JoinTypeOpt> for JoinKind {
    type Error = AnalysisError;

    fn try_from(value: JoinTypeOpt) -> Result<Self, Self::Error> {
        let Some(join_ty) = value.as_inner() else {
            AnalysisError::raise_unexpected_none("join_type")?
        };
        match join_ty {
            JoinType::JoinInner => Ok(Self::Inner),
            JoinType::JoinLeft => Ok(Self::Left),
            JoinType::JoinFull => Ok(Self::Full),
            JoinType::JoinRight => Ok(Self::Right),
            JoinType::JoinSemi => Ok(Self::Semi),
            JoinType::JoinAnti => Ok(Self::Anti),
            JoinType::JoinRightAnti => Ok(Self::RightAnti),
            JoinType::JoinUniqueOuter => Ok(Self::UniqueOuter),
            JoinType::JoinUniqueInner => Ok(Self::UniqueInner),
            JoinType::Undefined | JoinType::OptionNone => {
                AnalysisError::raise_unexpected_none("join_type")?
            }
        }
    }
}

impl JoinKind {
    /// get left nullability
    #[must_use]
    pub fn left_nullable(&self, in_nullable: Nullable) -> Nullable {
        match self {
            Self::Inner => in_nullable,
            Self::Left => in_nullable,
            Self::Right => true,
            Self::Full => true,
            Self::Semi => in_nullable,
            Self::Anti => in_nullable,
            Self::RightAnti => false,
            Self::UniqueInner => in_nullable,
            Self::UniqueOuter => true,
        }
    }

    /// get right nullability
    #[must_use]
    pub fn right_nullable(&self, in_nullable: Nullable) -> Nullable {
        match self {
            Self::Inner => in_nullable,
            Self::Left => true,
            Self::Right => in_nullable,
            Self::Full => true,
            Self::Semi => in_nullable,
            Self::Anti => in_nullable,
            Self::RightAnti => false,
            Self::UniqueInner => in_nullable,
            Self::UniqueOuter => true,
        }
    }
}
