use turso_parser::ast::ResolveType as ResolveTypeIn;

/// resolve type
#[derive(Clone, Debug)]
pub enum ResolveType {
    /// rollback
    Rollback,
    /// abort
    Abort,
    /// fail
    Fail,
    /// ignore
    Ignore,
    /// replace
    Replace,
}

impl From<ResolveTypeIn> for ResolveType {
    fn from(value: ResolveTypeIn) -> Self {
        match value {
            ResolveTypeIn::Rollback => Self::Rollback,
            ResolveTypeIn::Abort => Self::Abort,
            ResolveTypeIn::Fail => Self::Fail,
            ResolveTypeIn::Ignore => Self::Ignore,
            ResolveTypeIn::Replace => Self::Replace,
        }
    }
}
