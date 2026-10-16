use crate::{
    ast::TryFromWithTokens,
    sem::{
        AnalysisError, DeleteStatement, InsertStatement, ParseContext, SelectStatement,
        UpdateStatement,
        create_table::{ColumnDefinition, ColumnName},
    },
    syn::ScanToken,
};

use std::collections::HashMap;

use ::sql_fun_core::IVec;

/// CTE name
pub type CteName = String;

/// CTE query item types
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CteStatement {
    /// `SELECT` statement
    Select(Box<SelectStatement>),
    /// `INSERT` statement
    Insert(InsertStatement),
    /// `UPDATE` statement
    Update(UpdateStatement),
    /// `DELETE` statement
    Delete(DeleteStatement),
}

impl CteStatement {
    pub(crate) const fn insert() -> Self {
        Self::Insert(InsertStatement::empty())
    }
    pub(crate) const fn update() -> Self {
        Self::Update(UpdateStatement::empty())
    }
    pub(crate) const fn delete() -> Self {
        Self::Delete(DeleteStatement::empty())
    }

    /// get column from result set
    pub fn get_column<TParseContext>(
        &self,
        _context: &TParseContext,
        _name: &ColumnName,
    ) -> Option<ColumnDefinition>
    where
        TParseContext: ParseContext,
    {
        todo!()
    }

    /// get column at index
    pub fn get_column_at<TParseContext>(
        &self,
        _context: &TParseContext,
        _index: usize,
    ) -> Option<&ColumnDefinition>
    where
        TParseContext: ParseContext,
    {
        todo!()
    }

    /// get column definition with skip count
    pub fn get_column_def_with_skip_count<TParseContext>(
        &self,
        _context: &TParseContext,
        _name: &ColumnName,
        _skip_count: usize,
    ) -> Option<&ColumnDefinition>
    where
        TParseContext: ParseContext,
    {
        todo!()
    }

    /// test column existing
    pub(crate) fn has_column(&self, column: &ColumnName) -> bool {
        match self {
            Self::Select(query) => query.has_column(column),
            Self::Insert(_) | Self::Update(_) | Self::Delete(_) => false,
        }
    }

    pub(crate) fn from_query(query: SelectStatement) -> Self {
        Self::Select(Box::new(query))
    }

    pub(crate) fn resolved_column(
        &self,
        column: &ColumnName,
        index: Option<usize>,
        skip: usize,
    ) -> Option<ColumnDefinition> {
        match self {
            Self::Select(query) => {
                let columns = query.result_columns();
                let result = if let Some(index) = index {
                    columns.get_column_at(index)
                } else {
                    columns.get_column_with_skip_count(column, skip)
                };
                result.map(|column| column.create_column_definition())
            }
            Self::Insert(_) | Self::Update(_) | Self::Delete(_) => None,
        }
    }
}

/// analyzed [`crate::syn::WithClauseOpt`]
#[derive(Debug, Clone, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WithClause(HashMap<CteName, CteStatement>);

impl std::hash::Hash for WithClause {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (k, v) in &self.0 {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl WithClause {
    pub(crate) fn take(&mut self, name: &str) -> Option<CteStatement> {
        self.0.remove(name)
    }
    /// get CTE query by name
    #[must_use]
    pub fn get(&self, name: &CteName) -> Option<&CteStatement> {
        self.0.get(name)
    }

    /// insert named CTE item
    pub fn insert(&mut self, name: &CteName, statement: CteStatement) {
        self.0.insert(name.clone(), statement);
    }

    /// analyze [`crate::syn::WithClauseOpt`]
    pub fn analyze<TParseContext>(
        mut context: TParseContext,
        syn: crate::syn::WithClauseOpt,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::With::try_from_with_tokens(syn, tokens)?;
        let result = syntax.resolve_with(&context, crate::ast::Dialect::postgres());
        Ok((result.report(&mut context)?, context))
    }
}

impl CteStatement {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        match self {
            Self::Select(query) => query.walk(visitor)?,
            Self::Insert(_) | Self::Update(_) | Self::Delete(_) => {}
        }
        Ok(())
    }
}

impl WithClause {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        for (name, query) in &mut self.0 {
            query.walk(visitor)?;
            visitor.cte(name);
        }
        Ok(())
    }
}
