use crate::{
    ast::TryCollectFromWithTokens,
    sem::{
        AliasSpec, AnalysisError, DataSource, Nullable, ParseContext, WithClause,
        create_table::ColumnName, data_source::AliasName,
    },
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

/// semantic representation for `FROM` clause
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FromClause(Vec<DataSource>);

impl FromClause {
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
        let result = crate::ast::FromItem::resolve_list(
            &syntax,
            &context,
            crate::ast::Dialect::postgres(),
            with_clause,
        );
        Ok((result.report(&mut context)?, context))
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

    pub(crate) fn find_alias_resolved<TContext: crate::sem::resolution::ResolutionContext>(
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
