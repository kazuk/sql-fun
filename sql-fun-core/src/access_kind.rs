//! SQL access purposes shared by semantic analysis and its wire consumers.
/// Why a top-level SQL operation needs an object or column.
/// Nested expressions and subqueries inherit their enclosing purpose.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AccessKind {
    /// Insert rows into a relation.
    InsertRows,
    /// Read rows from a SQL-visible source.
    ReadRows,
    /// Update rows in a relation.
    UpdateRows,
    /// Delete rows from a relation.
    DeleteRows,
    /// Produce SELECT output.
    SelectOutput,
    /// Evaluate a SELECT source expression.
    SelectSource,
    /// Filter SELECT rows.
    SelectFilter,
    /// Match joined SELECT rows.
    SelectJoin,
    /// Form SELECT groups.
    SelectGrouping,
    /// Filter SELECT groups.
    SelectGroupFilter,
    /// Order SELECT output.
    SelectOrdering,
    /// Choose distinct SELECT rows.
    SelectDistinct,
    /// Define a named SELECT window.
    SelectWindow,
    /// Limit SELECT output rows.
    SelectLimit,
    /// Skip SELECT output rows.
    SelectOffset,
    /// Write an INSERT destination column.
    InsertTarget,
    /// Supply inserted values.
    InsertInput,
    /// Produce INSERT RETURNING output.
    InsertReturning,
    /// Evaluate INSERT conflict selection.
    InsertConflict,
    /// Write an UPDATE assignment destination.
    UpdateTarget,
    /// Compute an UPDATE assignment input.
    UpdateInput,
    /// Evaluate an UPDATE source expression.
    UpdateSource,
    /// Filter updated rows.
    UpdateFilter,
    /// Match joined UPDATE rows.
    UpdateJoin,
    /// Produce UPDATE RETURNING output.
    UpdateReturning,
    /// Evaluate a DELETE source expression.
    DeleteSource,
    /// Filter deleted rows.
    DeleteFilter,
    /// Match joined DELETE rows.
    DeleteJoin,
    /// Produce DELETE RETURNING output.
    DeleteReturning,
    /// Evaluate a MERGE source.
    MergeSource,
    /// Match MERGE source and target rows.
    MergeJoin,
    /// Choose a MERGE action.
    MergeCondition,
    /// Produce MERGE RETURNING output.
    MergeReturning,
}
impl AccessKind {
    /// Insert rows into a relation.
    #[must_use]
    pub const fn insert_rows() -> Self {
        Self::InsertRows
    }
    /// Read rows from a SQL-visible source.
    #[must_use]
    pub const fn read_rows() -> Self {
        Self::ReadRows
    }
    /// Update rows in a relation.
    #[must_use]
    pub const fn update_rows() -> Self {
        Self::UpdateRows
    }
    /// Delete rows from a relation.
    #[must_use]
    pub const fn delete_rows() -> Self {
        Self::DeleteRows
    }
    /// Produce SELECT output.
    #[must_use]
    pub const fn select_output() -> Self {
        Self::SelectOutput
    }
    /// Evaluate a SELECT source expression.
    #[must_use]
    pub const fn select_source() -> Self {
        Self::SelectSource
    }
    /// Filter SELECT rows.
    #[must_use]
    pub const fn select_filter() -> Self {
        Self::SelectFilter
    }
    /// Match joined SELECT rows.
    #[must_use]
    pub const fn select_join() -> Self {
        Self::SelectJoin
    }
    /// Form SELECT groups.
    #[must_use]
    pub const fn select_grouping() -> Self {
        Self::SelectGrouping
    }
    /// Filter SELECT groups.
    #[must_use]
    pub const fn select_group_filter() -> Self {
        Self::SelectGroupFilter
    }
    /// Order SELECT output.
    #[must_use]
    pub const fn select_ordering() -> Self {
        Self::SelectOrdering
    }
    /// Choose distinct SELECT rows.
    #[must_use]
    pub const fn select_distinct() -> Self {
        Self::SelectDistinct
    }
    /// Define a named SELECT window.
    #[must_use]
    pub const fn select_window() -> Self {
        Self::SelectWindow
    }
    /// Limit SELECT output rows.
    #[must_use]
    pub const fn select_limit() -> Self {
        Self::SelectLimit
    }
    /// Skip SELECT output rows.
    #[must_use]
    pub const fn select_offset() -> Self {
        Self::SelectOffset
    }
    /// Write an INSERT destination column.
    #[must_use]
    pub const fn insert_target() -> Self {
        Self::InsertTarget
    }
    /// Supply inserted values.
    #[must_use]
    pub const fn insert_input() -> Self {
        Self::InsertInput
    }
    /// Produce INSERT RETURNING output.
    #[must_use]
    pub const fn insert_returning() -> Self {
        Self::InsertReturning
    }
    /// Evaluate INSERT conflict selection.
    #[must_use]
    pub const fn insert_conflict() -> Self {
        Self::InsertConflict
    }
    /// Write an UPDATE assignment destination.
    #[must_use]
    pub const fn update_target() -> Self {
        Self::UpdateTarget
    }
    /// Compute an UPDATE assignment input.
    #[must_use]
    pub const fn update_input() -> Self {
        Self::UpdateInput
    }
    /// Evaluate an UPDATE source expression.
    #[must_use]
    pub const fn update_source() -> Self {
        Self::UpdateSource
    }
    /// Filter updated rows.
    #[must_use]
    pub const fn update_filter() -> Self {
        Self::UpdateFilter
    }
    /// Match joined UPDATE rows.
    #[must_use]
    pub const fn update_join() -> Self {
        Self::UpdateJoin
    }
    /// Produce UPDATE RETURNING output.
    #[must_use]
    pub const fn update_returning() -> Self {
        Self::UpdateReturning
    }
    /// Evaluate a DELETE source expression.
    #[must_use]
    pub const fn delete_source() -> Self {
        Self::DeleteSource
    }
    /// Filter deleted rows.
    #[must_use]
    pub const fn delete_filter() -> Self {
        Self::DeleteFilter
    }
    /// Match joined DELETE rows.
    #[must_use]
    pub const fn delete_join() -> Self {
        Self::DeleteJoin
    }
    /// Produce DELETE RETURNING output.
    #[must_use]
    pub const fn delete_returning() -> Self {
        Self::DeleteReturning
    }
    /// Evaluate a MERGE source.
    #[must_use]
    pub const fn merge_source() -> Self {
        Self::MergeSource
    }
    /// Match MERGE source and target rows.
    #[must_use]
    pub const fn merge_join() -> Self {
        Self::MergeJoin
    }
    /// Choose a MERGE action.
    #[must_use]
    pub const fn merge_condition() -> Self {
        Self::MergeCondition
    }
    /// Produce MERGE RETURNING output.
    #[must_use]
    pub const fn merge_returning() -> Self {
        Self::MergeReturning
    }
    /// Stable SQL/application-facing name used in serialized records.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::InsertRows => "InsertRows",
            Self::ReadRows => "ReadRows",
            Self::UpdateRows => "UpdateRows",
            Self::DeleteRows => "DeleteRows",
            Self::SelectOutput => "SelectOutput",
            Self::SelectSource => "SelectSource",
            Self::SelectFilter => "SelectFilter",
            Self::SelectJoin => "SelectJoin",
            Self::SelectGrouping => "SelectGrouping",
            Self::SelectGroupFilter => "SelectGroupFilter",
            Self::SelectOrdering => "SelectOrdering",
            Self::SelectDistinct => "SelectDistinct",
            Self::SelectWindow => "SelectWindow",
            Self::SelectLimit => "SelectLimit",
            Self::SelectOffset => "SelectOffset",
            Self::InsertTarget => "InsertTarget",
            Self::InsertInput => "InsertInput",
            Self::InsertReturning => "InsertReturning",
            Self::InsertConflict => "InsertConflict",
            Self::UpdateTarget => "UpdateTarget",
            Self::UpdateInput => "UpdateInput",
            Self::UpdateSource => "UpdateSource",
            Self::UpdateFilter => "UpdateFilter",
            Self::UpdateJoin => "UpdateJoin",
            Self::UpdateReturning => "UpdateReturning",
            Self::DeleteSource => "DeleteSource",
            Self::DeleteFilter => "DeleteFilter",
            Self::DeleteJoin => "DeleteJoin",
            Self::DeleteReturning => "DeleteReturning",
            Self::MergeSource => "MergeSource",
            Self::MergeJoin => "MergeJoin",
            Self::MergeCondition => "MergeCondition",
            Self::MergeReturning => "MergeReturning",
        }
    }
    /// Whether this purpose writes a column rather than reads its value.
    #[must_use]
    pub const fn is_column_target(self) -> bool {
        matches!(self, Self::InsertTarget | Self::UpdateTarget)
    }
    /// Purpose of a join predicate within this statement's source clause.
    #[must_use]
    pub const fn join_purpose(self) -> Self {
        match self {
            Self::UpdateSource | Self::UpdateJoin => Self::UpdateJoin,
            Self::DeleteSource | Self::DeleteJoin => Self::DeleteJoin,
            Self::MergeSource | Self::MergeJoin => Self::MergeJoin,
            _ => Self::SelectJoin,
        }
    }
}
