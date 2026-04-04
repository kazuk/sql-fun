use sql_fun_core::IVec;

use crate::{
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, DataSource, Nullable, ParseContext, WithClause,
        create_table::ColumnName, data_source::AliasName,
    },
    syn::{ListOpt, ScanToken},
};

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
        let Some(nodes) = syn.as_inner() else {
            AnalysisError::raise_unexpected_none("from_clause.as_inner")?
        };
        let mut from_clause = Self::default();
        let mut existing_aliases = Vec::new();

        for node in nodes {
            let (source, new_context) = DataSource::analyze_from_clause_node(
                context,
                with_clause,
                &from_clause,
                node,
                tokens,
            )?;
            context = new_context;

            let aliases = source.get_aliases();
            for alias in aliases {
                if existing_aliases.contains(&alias) {
                    context.report_problem(AnalysisProblem::duplicate_alias(alias.name()))?;
                }
                existing_aliases.push(alias);
            }
            from_clause.push(source);
        }
        Ok((from_clause, context))
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
        let mut aliases = Vec::new();

        for source in &self.0 {
            let mut results = source.get_aliases_for_column(column, context, with_clause);
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
