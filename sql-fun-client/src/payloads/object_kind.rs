/// object kind.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ObjectKind {
    /// All object kinds.
    All,
    /// table
    Table,
    /// view
    View,
    /// type
    Type,
    /// function
    Function,
    /// materialized view
    MaterializedView,
    /// sequence
    Sequence,
    /// trigger
    Trigger,
    /// index
    Index,
}

impl ObjectKind {
    /// An ordinary table.
    #[must_use]
    pub const fn table() -> Self {
        Self::Table
    }
    /// An ordinary view.
    #[must_use]
    pub const fn view() -> Self {
        Self::View
    }
    /// A materialized view.
    #[must_use]
    pub const fn materialized_view() -> Self {
        Self::MaterializedView
    }
    /// A function.
    #[must_use]
    pub const fn function() -> Self {
        Self::Function
    }
}
