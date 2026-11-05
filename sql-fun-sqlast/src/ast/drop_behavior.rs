use crate::{ast::DropBehavior, error::AnalysisError, syn};

impl DropBehavior {
    pub(super) const fn cascade() -> Self {
        Self::Cascade
    }

    pub(super) const fn restrict() -> Self {
        Self::Restrict
    }
}

impl TryFrom<Option<syn::DropBehavior>> for DropBehavior {
    type Error = AnalysisError;

    fn try_from(value: Option<syn::DropBehavior>) -> Result<Self, Self::Error> {
        match value {
            Some(syn::DropBehavior::DropCascade) => Ok(Self::cascade()),
            Some(syn::DropBehavior::DropRestrict) => Ok(Self::restrict()),
            _ => AnalysisError::raise_unexpected_input("DROP behavior"),
        }
    }
}
