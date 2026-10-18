use crate::{
    ast::TryCollectFromWithTokens,
    sem::{
        AliasSpec, AnalysisError, DataSource, MergeMatchKind, Nullable, ParseContext, WithClause,
        create_table::ColumnName, data_source::AliasName, data_source::MergeRecordRole,
    },
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

/// semantic representation for `FROM` clause
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FromClause(Vec<DataSource>);

#[derive(Clone, Copy)]
struct MergeScopeConfig {
    include_target: bool,
    include_source: bool,
    target_nullable: bool,
    source_nullable: bool,
    row_images: Option<MergeRowImageNullability>,
    merge_action_available: bool,
}

#[derive(Clone, Copy)]
struct MergeRowImageNullability {
    old: bool,
    new: bool,
}

impl FromClause {
    pub(crate) fn for_merge_on(target: &DataSource, source: &DataSource) -> Self {
        Self::merge_scope(target, source, MergeScopeConfig::on())
    }

    pub(crate) fn for_merge_when(
        target: &DataSource,
        source: &DataSource,
        kind: MergeMatchKind,
    ) -> Self {
        Self::merge_scope(target, source, MergeScopeConfig::when(kind))
    }

    pub(crate) fn for_merge_returning(
        target: &DataSource,
        source: &DataSource,
        source_nullable: bool,
        old_nullable: bool,
        new_nullable: bool,
        row_images_enabled: bool,
    ) -> Self {
        Self::merge_scope(
            target,
            source,
            MergeScopeConfig::returning(
                source_nullable,
                old_nullable,
                new_nullable,
                row_images_enabled,
            ),
        )
    }

    fn merge_scope(target: &DataSource, source: &DataSource, config: MergeScopeConfig) -> Self {
        let mut sources = Vec::new();
        if config.include_source {
            sources.push(DataSource::provided_merge_record(
                source,
                MergeRecordRole::source(),
                config.source_nullable,
                config.merge_action_available,
                false,
                false,
                false,
            ));
        }
        if config.include_target {
            let row_images = config.row_images.unwrap_or(MergeRowImageNullability {
                old: false,
                new: false,
            });
            sources.push(DataSource::provided_merge_record(
                target,
                MergeRecordRole::target(),
                config.target_nullable,
                config.merge_action_available,
                row_images.old,
                row_images.new,
                config.row_images.is_some(),
            ));
        }
        Self(sources)
    }

    pub(crate) fn merge_action_available(&self) -> bool {
        self.0.iter().any(DataSource::merge_action_available)
    }

    pub(crate) fn merge_row_target(
        &self,
        alias: &crate::sem::data_source::AliasName,
    ) -> Option<(&crate::sem::data_source::AliasName, MergeRecordRole, bool)> {
        if self.resolve_alias(alias).is_some() {
            return None;
        }
        self.0
            .iter()
            .find_map(|source| source.merge_row_target(alias))
    }

    pub(crate) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<(AliasName, ColumnName)>, AnalysisError> {
        let mut names = Vec::new();
        for source in &self.0 {
            names.extend(source.projection_names(context, with, qualifier)?);
        }
        Ok(names)
    }
    /// add data source
    pub fn push(&mut self, source: DataSource) {
        self.0.push(source);
    }

    /// analyze from-cluse [`crate::syn::NodeListOpt`]
    pub fn analyze<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        syn: crate::syn::NodeListOpt,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::FromItem::try_collect_from_with_tokens(syn, tokens)?;
        let result = crate::ast::FromItem::resolve_list(&syntax, &mut context, with_clause);
        Ok((result?, context))
    }

    /// get data-source by alias name
    #[must_use]
    pub fn resolve_alias(&self, alias_name: &AliasName) -> Option<(&DataSource, Nullable)> {
        for source in &self.0 {
            if let Some(resolved) = source.resolve_alias(alias_name) {
                return Some(resolved);
            }
        }
        None
    }

    /// find alias name by column name
    pub fn find_alias_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
    ) -> Result<Option<AliasSpec>, Vec<AliasSpec>>
    where
        TParseContext: ParseContext,
    {
        self.find_alias_resolved(column, context, with_clause)
    }

    pub(crate) fn find_alias_resolved<TContext: crate::sem::ParseContext>(
        &self,
        column: &ColumnName,
        context: &TContext,
        with_clause: &WithClause,
    ) -> Result<Option<AliasSpec>, Vec<AliasSpec>> {
        let mut aliases = Vec::new();

        for source in &self.0 {
            let mut results = source.aliases_for_column_resolved(column, context, with_clause);
            aliases.append(&mut results);
        }

        if aliases.is_empty() {
            Ok(None)
        } else if aliases.len() == 1 {
            Ok(Some(aliases[0].clone()))
        } else {
            Err(aliases)
        }
    }
}

impl MergeScopeConfig {
    const fn on() -> Self {
        Self {
            include_target: true,
            include_source: true,
            target_nullable: false,
            source_nullable: false,
            row_images: None,
            merge_action_available: false,
        }
    }

    const fn when(kind: MergeMatchKind) -> Self {
        Self {
            include_target: kind.has_target(),
            include_source: kind.has_source(),
            target_nullable: false,
            source_nullable: false,
            row_images: None,
            merge_action_available: false,
        }
    }

    const fn returning(
        source_nullable: bool,
        old_nullable: bool,
        new_nullable: bool,
        row_images_enabled: bool,
    ) -> Self {
        Self {
            include_target: true,
            include_source: true,
            target_nullable: false,
            source_nullable,
            row_images: if row_images_enabled {
                Some(MergeRowImageNullability {
                    old: old_nullable,
                    new: new_nullable,
                })
            } else {
                None
            },
            merge_action_available: true,
        }
    }
}

impl FromClause {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        for source in &mut self.0 {
            source.walk(visitor)?;
        }
        Ok(())
    }
}
