mod ast;
mod destination;
mod view_support;

pub(crate) use self::destination::MergeDestination;
pub use self::view_support::{ViewMergeSupport, ViewMergeTriggers};
use crate::{
    ast::Dialect,
    names::{ColumnName, TableName},
    sem::{
        AccessKind, AnalysisError, ColumnDefinition, FromClause, InsertClauseSemantics,
        InsertTarget, InsertValue, ParseContext, QueryCte, ResultColumnCollection, SemScalarExpr,
        UpdateAssignment, UpdateAssignmentCollection, ViewMergeSupport as SemViewMergeSupport,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// The rows for which a MERGE action is eligible.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MergeMatchKind {
    /// Target and source both exist.
    Matched,
    /// Only the target exists.
    NotMatchedBySource,
    /// Only the source exists.
    NotMatchedByTarget,
}
impl MergeMatchKind {
    pub(crate) const fn matched() -> Self {
        Self::Matched
    }
    pub(crate) const fn by_source() -> Self {
        Self::NotMatchedBySource
    }
    pub(crate) const fn by_target() -> Self {
        Self::NotMatchedByTarget
    }
    /// Whether the source row is visible.
    pub const fn has_source(self) -> bool {
        !matches!(self, Self::NotMatchedBySource)
    }
    /// Whether the target row is visible.
    pub const fn has_target(self) -> bool {
        !matches!(self, Self::NotMatchedByTarget)
    }
}

/// A MERGE INSERT clause with independently resolved destination and values.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeInsert {
    clause: InsertClauseSemantics,
    overriding: Option<String>,
}
impl MergeInsert {
    /// Destination columns in SQL order.
    pub fn targets(&self) -> &[InsertTarget] {
        self.clause.targets().as_slice()
    }
    /// Explicit VALUES, absent for DEFAULT VALUES.
    pub fn values(&self) -> Option<&[InsertValue]> {
        self.clause.values().values().map(|v| v.as_slice())
    }
    /// Explicit identity override mode.
    pub fn overriding(&self) -> Option<&str> {
        self.overriding.as_deref()
    }
}

/// A typed MERGE action; it does not contain a standalone DML statement.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MergeAction {
    /// A diagnosed action whose semantics could not be resolved.
    Unresolved,
    /// UPDATE assignments.
    Update(MergeUpdate),
    /// INSERT clause.
    Insert(MergeInsert),
    /// DELETE the matched target.
    Delete,
    /// Leave the row unchanged.
    Nothing,
}
/// UPDATE assignments belonging to a MERGE action.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeUpdate(UpdateAssignmentCollection);
impl MergeUpdate {
    /// Assignments in SQL order.
    pub fn assignments(&self) -> &[UpdateAssignment] {
        self.0.as_slice()
    }
}
impl MergeAction {
    pub(crate) const fn unresolved() -> Self {
        Self::Unresolved
    }
    pub(crate) fn update(assignments: &UpdateAssignmentCollection) -> Self {
        Self::Update(MergeUpdate(assignments.clone()))
    }
    pub(crate) fn insert(clause: &InsertClauseSemantics, overriding: Option<&str>) -> Self {
        Self::Insert(MergeInsert {
            clause: clause.clone(),
            overriding: overriding.map(str::to_owned),
        })
    }
    pub(crate) const fn delete() -> Self {
        Self::Delete
    }
    pub(crate) const fn nothing() -> Self {
        Self::Nothing
    }
    /// UPDATE payload, when applicable.
    pub const fn as_update(&self) -> Option<&MergeUpdate> {
        if let Self::Update(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// INSERT payload, when applicable.
    pub const fn as_insert(&self) -> Option<&MergeInsert> {
        if let Self::Insert(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// Whether this action deletes the target.
    pub const fn is_delete(&self) -> bool {
        matches!(self, Self::Delete)
    }
    /// Whether this action changes no row.
    pub const fn is_nothing(&self) -> bool {
        matches!(self, Self::Nothing)
    }
    const fn access_action(&self) -> Option<AccessKind> {
        match self {
            Self::Update(_) => Some(AccessKind::update_rows()),
            Self::Insert(_) => Some(AccessKind::insert_rows()),
            Self::Delete => Some(AccessKind::delete_rows()),
            Self::Nothing | Self::Unresolved => None,
        }
    }
    fn walk<V: SemanticVisitorMut>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Update(value) => value.0.walk(visitor),
            Self::Insert(value) => value.clause.walk(visitor),
            Self::Unresolved | Self::Delete | Self::Nothing => Ok(()),
        }
    }
    fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Update(value) => value.0.visit(visitor),
            Self::Insert(value) => value.clause.visit(visitor),
            Self::Unresolved | Self::Delete | Self::Nothing => Ok(()),
        }
    }
    fn validate(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        match self {
            Self::Update(value) => value.0.validate(context),
            Self::Insert(value) => value.clause.validate_assignments(context),
            Self::Unresolved | Self::Delete | Self::Nothing => Ok(()),
        }
    }
}
/// One ordered WHEN clause.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeWhen {
    kind: MergeMatchKind,
    condition: Option<SemScalarExpr>,
    action: MergeAction,
}
impl MergeWhen {
    pub(crate) fn new(
        kind: MergeMatchKind,
        condition: &Option<SemScalarExpr>,
        action: &MergeAction,
    ) -> Self {
        Self {
            kind,
            condition: condition.clone(),
            action: action.clone(),
        }
    }
    /// Match category.
    pub const fn match_kind(&self) -> MergeMatchKind {
        self.kind
    }
    /// Additional boolean condition.
    pub const fn condition(&self) -> Option<&SemScalarExpr> {
        self.condition.as_ref()
    }
    /// Typed action.
    pub const fn action(&self) -> &MergeAction {
        &self.action
    }
}
/// Ordered MERGE clauses.
#[derive(Debug, Default, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct MergeWhenCollection(Vec<MergeWhen>);
impl MergeWhenCollection {
    pub(crate) fn push(&mut self, clause: &MergeWhen) {
        self.0.push(clause.clone());
    }
    pub(crate) fn permits_insert(&self) -> bool {
        self.0.iter().any(|c| c.action.as_insert().is_some())
    }
    pub(crate) fn permits_update(&self) -> bool {
        self.0.iter().any(|c| c.action.as_update().is_some())
    }
    pub(crate) fn permits_delete(&self) -> bool {
        self.0.iter().any(|c| c.action.is_delete())
    }
    pub(crate) fn source_nullable(&self) -> bool {
        self.0
            .iter()
            .any(|c| !c.kind.has_source() && !c.action.is_nothing())
    }
    fn walk<V: SemanticVisitorMut>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        for clause in &mut self.0 {
            if let Some(condition) = &mut clause.condition {
                condition.walk(visitor)?;
                visitor.predicate(condition);
            }
            clause.action.walk(visitor)?;
        }
        Ok(())
    }
    fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for clause in &self.0 {
            visitor.access_purpose(AccessKind::merge_condition());
            if let Some(condition) = &clause.condition {
                condition.visit(visitor)?;
                visitor.predicate(condition);
            }
            clause.action.visit(visitor)?;
        }
        Ok(())
    }
    fn validate(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        for clause in &mut self.0 {
            clause.action.validate(context)?;
        }
        Ok(())
    }
}
/// An analyzed PostgreSQL MERGE statement.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeStatement {
    target: TableName,
    with: Vec<QueryCte>,
    from: FromClause,
    on: SemScalarExpr,
    clauses: MergeWhenCollection,
    returning: ResultColumnCollection,
    inherit: bool,
    view_support: Option<SemViewMergeSupport>,
}
impl MergeStatement {
    pub(crate) fn require_version(
        context: &impl ParseContext,
        minimum: u32,
    ) -> Result<(), AnalysisError> {
        AnalysisError::check_dialect(Dialect::postgres(), context.dialect())?;
        let version = if let Some(version) = context.metadata().engine_version() {
            version
                .to_string()
                .parse::<::sql_fun_core::PostgresVersion>()
                .ok()
        } else {
            None
        };
        if version.is_none_or(|v| v.major() < minimum) {
            return Err(AnalysisError::unexpected_input_error(&format!(
                "MERGE requires PostgreSQL {minimum} or later"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_respects_versions {
    use crate::test_helpers::{document_merge::catalog, statements::resolve_postgres as resolve};

    #[rstest::rstest]
    #[case::basic15(
        15,
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE",
        true
    )]
    #[case::basic16(
        16,
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE",
        true
    )]
    #[case::old14(
        14,
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE",
        false
    )]
    #[case::returning16(
        16,
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id",
        false
    )]
    #[case::by_source16(
        16,
        "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED BY SOURCE THEN DELETE",
        false
    )]
    #[case::cte16(
        16,
        "WITH m AS (MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE) SELECT 1",
        false
    )]
    #[case::cte17(
        17,
        "WITH m AS (MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id) SELECT id FROM m",
        true
    )]
    #[case::old17(
        17,
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING old.id",
        false
    )]
    #[case::old18(
        18,
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING old.id,new.id",
        true
    )]
    fn respects_versions(#[case] version: u32, #[case] sql: &str, #[case] accepted: bool) {
        let mut context = catalog(version);
        let result = resolve(sql, &mut context);
        assert_eq!(
            result.is_ok() && context.reported_problem_count() == 0,
            accepted,
            "{result:?} {:?}",
            context.reported_problems()
        );
    }
}

impl MergeStatement {
    pub(crate) fn new(
        target: &TableName,
        with: &[QueryCte],
        from: &FromClause,
        on: &SemScalarExpr,
        clauses: &MergeWhenCollection,
        returning: &ResultColumnCollection,
        inherit: bool,
    ) -> Self {
        Self {
            target: target.clone(),
            with: with.to_vec(),
            from: from.clone(),
            on: on.clone(),
            clauses: clauses.clone(),
            returning: returning.clone(),
            inherit,
            view_support: None,
        }
    }
    pub(crate) fn set_view_support(&mut self, support: Option<&SemViewMergeSupport>) {
        self.view_support = support.cloned();
    }
    /// MERGE-specific view capability decision, when targeting an ordinary view.
    pub const fn view_support(&self) -> Option<&SemViewMergeSupport> {
        self.view_support.as_ref()
    }
    /// Resolved catalog target.
    pub const fn target(&self) -> &TableName {
        &self.target
    }
    /// Local CTE definitions.
    pub fn with(&self) -> &[QueryCte] {
        &self.with
    }
    /// Resolved USING and target records.
    pub const fn from(&self) -> &FromClause {
        &self.from
    }
    /// Boolean ON expression.
    pub const fn on(&self) -> &SemScalarExpr {
        &self.on
    }
    /// WHEN clauses in SQL order.
    pub fn clauses(&self) -> &[MergeWhen] {
        &self.clauses.0
    }
    /// Statement-wide RETURNING outputs.
    pub const fn result_columns(&self) -> &ResultColumnCollection {
        &self.returning
    }
    /// Whether descendants are included.
    pub const fn includes_descendants(&self) -> bool {
        self.inherit
    }
    pub(crate) fn rename_columns(&mut self, names: &[String]) {
        self.returning.rename(names);
    }
    pub(crate) fn validate_assignments(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.clauses.validate(context)
    }
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.enter_dml();
        visitor.enter_query();
        let result = self.walk_body(visitor);
        visitor.leave_query();
        visitor.leave_dml();
        result
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.enter_dml();
        visitor.enter_query();
        let result = self.visit_body(visitor);
        visitor.leave_query();
        visitor.leave_dml();
        result
    }
    fn walk_body<V: SemanticVisitorMut>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        for cte in &mut self.with {
            cte.walk(visitor)?;
        }
        self.from.walk(visitor)?;
        self.on.walk(visitor)?;
        visitor.predicate(&self.on);
        // PostgreSQL transforms RETURNING before WHEN action expressions.
        visitor.dml_returning(&mut self.returning)?;
        self.clauses.walk(visitor)
    }
    fn visit_body<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.access_purpose(AccessKind::merge_source());
        for cte in &self.with {
            cte.visit(visitor)?;
        }
        for clause in &self.clauses.0 {
            let kind = clause.action.access_action();
            if let Some(kind) = kind {
                visitor.access_action(self.target.full_name(), kind);
            }
        }
        self.from.visit(visitor)?;
        visitor.access_purpose(AccessKind::merge_join());
        self.on.visit(visitor)?;
        visitor.predicate(&self.on);
        // PostgreSQL transforms RETURNING before WHEN action expressions.
        visitor.access_purpose(AccessKind::merge_returning());
        visitor.dml_returning(&self.returning)?;
        self.clauses.visit(visitor)
    }
}

impl MergeStatement {
    pub(crate) fn resolved_column(
        column: &ColumnName,
        index: Option<usize>,
        skip: usize,
        merge: &Self,
    ) -> Option<ColumnDefinition> {
        let columns = merge.result_columns();
        let result = if let Some(index) = index {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, skip)
        };
        result.map(|column| column.create_column_definition())
    }
}
