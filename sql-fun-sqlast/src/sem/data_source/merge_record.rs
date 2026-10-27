use crate::{
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, ColumnName, DataSource, Nullable, SemScalarExpr, WithClause,
        data_source::AliasName, scalar_expr::ColumnReferenceExpr,
    },
};

/// The record exposed by a MERGE scope.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MergeRecordRole {
    /// The target relation row.
    Target,
    /// The USING source row.
    Source,
    /// The target row before the MERGE action.
    Old,
    /// The target row after the MERGE action.
    New,
}

impl MergeRecordRole {
    /// The target row used by MERGE.
    pub const fn target() -> Self {
        Self::Target
    }
    /// The USING source row used by MERGE.
    pub const fn source() -> Self {
        Self::Source
    }
    /// The target row before the action.
    pub const fn old_row() -> Self {
        Self::Old
    }
    /// The target row after the action.
    pub const fn new_row() -> Self {
        Self::New
    }
    const fn is_target(self) -> bool {
        matches!(self, Self::Target)
    }
}

/// A MERGE target or source record retaining its original relation binding.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeRecordDataSource {
    source: Box<DataSource>,
    role: MergeRecordRole,
    nullable: Nullable,
    merge_action_available: bool,
    target_alias: Option<AliasName>,
    old_nullable: bool,
    new_nullable: bool,
    row_images_enabled: bool,
}

impl MergeRecordDataSource {
    pub(super) fn qualified_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &crate::sem::FullName,
    ) -> Result<Option<super::QualifiedSource<'a>>, ()> {
        Ok(self
            .source
            .qualified_source(context, name)?
            .map(|source| source.with_merge(self.role, self.nullable)))
    }

    pub(crate) fn new(
        source: &DataSource,
        role: MergeRecordRole,
        nullable: Nullable,
        merge_action_available: bool,
        old_nullable: bool,
        new_nullable: bool,
        row_images_enabled: bool,
    ) -> Self {
        let target_alias = if role.is_target() && row_images_enabled {
            source
                .get_aliases()
                .first()
                .map(|alias| alias.name().clone())
        } else {
            None
        };
        Self {
            source: Box::new(source.clone()),
            role,
            nullable,
            merge_action_available,
            target_alias,
            old_nullable,
            new_nullable,
            row_images_enabled,
        }
    }

    /// The target or source record represented by this data source.
    pub const fn role(&self) -> MergeRecordRole {
        self.role
    }

    /// The original semantic source whose columns are exposed by this record.
    pub fn source(&self) -> &DataSource {
        &self.source
    }

    /// Whether this record is nullable because MERGE may lack its row.
    pub const fn is_nullable(&self) -> bool {
        self.nullable
    }

    pub(crate) const fn merge_action_available(&self) -> bool {
        self.merge_action_available
    }

    pub(crate) fn merge_row_target(
        &self,
        alias: &AliasName,
    ) -> Option<(&AliasName, MergeRecordRole, bool)> {
        if !self.row_images_enabled || !self.role.is_target() {
            return None;
        }
        match alias.as_str() {
            "old" => Some((
                self.target_alias.as_ref()?,
                MergeRecordRole::old_row(),
                self.old_nullable,
            )),
            "new" => Some((
                self.target_alias.as_ref()?,
                MergeRecordRole::new_row(),
                self.new_nullable,
            )),
            _ => None,
        }
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        self.source.push_alias_names(names);
    }

    pub(super) fn resolve_alias<'a>(
        &self,
        wrapper: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        let (_, source_nullable) = self.source.resolve_alias(name)?;
        Some((wrapper, self.nullable || source_nullable))
    }

    pub(super) fn column_candidates<C: ParseContext>(
        &self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
    ) -> super::ColumnCandidates {
        self.source
            .aliases_for_column_resolved(column, context, with)
    }

    pub(super) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<super::ProjectionColumn<'_>>, AnalysisError> {
        Ok(self
            .source
            .projection_names(context, with, qualifier)?
            .into_iter()
            .map(|column| column.with_merge(self.role, self.nullable))
            .collect())
    }

    pub(super) fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        self.source.column_names(context, with)
    }

    pub(super) fn create_column_ref<'context, C: ParseContext>(
        &self,
        context: &'context mut C,
        with: &WithClause,
        alias: &AliasName,
        column: &ColumnName,
        nullable: Nullable,
    ) -> Result<(SemScalarExpr, &'context mut C), AnalysisError> {
        let Some((source, source_nullable)) = self.source.resolve_alias(alias) else {
            let expression = ColumnReferenceExpr::undefined_alias_resolved(context, alias, column)?;
            return Ok((expression, context));
        };
        let (expression, context) =
            source.create_column_ref(context, with, alias, column, nullable || source_nullable)?;
        let expression = expression.merge_record_column(self.role, nullable || source_nullable);
        Ok((expression, context))
    }

    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.source.walk(visitor)
    }
}

impl DataSource {
    pub(crate) fn provided_merge_record(
        source: &Self,
        role: MergeRecordRole,
        nullable: Nullable,
        merge_action_available: bool,
        old_nullable: bool,
        new_nullable: bool,
        row_images_enabled: bool,
    ) -> Self {
        Self::MergeRecord(Box::new(MergeRecordDataSource::new(
            source,
            role,
            nullable,
            merge_action_available,
            old_nullable,
            new_nullable,
            row_images_enabled,
        )))
    }
}

impl MergeRecordDataSource {
    pub(super) fn projection_shape_is_resolved(&self) -> bool {
        self.source().projection_shape_is_resolved()
    }

    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.source().projection_is_table_dependent(context)
    }
}
