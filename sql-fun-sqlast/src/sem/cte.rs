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
    Insert(Box<InsertStatement>),
    /// `UPDATE` statement
    Update(Box<UpdateStatement>),
    /// `DELETE` statement
    Delete(Box<DeleteStatement>),
}

impl CteStatement {
    /// get column from result set
    pub fn get_column<TParseContext>(
        &self,
        _context: &TParseContext,
        name: &ColumnName,
    ) -> Option<ColumnDefinition>
    where
        TParseContext: ParseContext,
    {
        self.resolved_column(name, None, 0)
    }

    /// get column at index
    pub fn get_column_at<TParseContext>(
        &self,
        _context: &TParseContext,
        index: usize,
    ) -> Option<ColumnDefinition>
    where
        TParseContext: ParseContext,
    {
        self.resolved_column(&ColumnName::from(""), Some(index), 0)
    }

    /// get column definition with skip count
    pub fn get_column_def_with_skip_count<TParseContext>(
        &self,
        _context: &TParseContext,
        name: &ColumnName,
        skip_count: usize,
    ) -> Option<ColumnDefinition>
    where
        TParseContext: ParseContext,
    {
        self.resolved_column(name, None, skip_count)
    }

    pub(crate) fn result_columns(&self) -> Option<&crate::sem::ResultColumnCollection> {
        match self {
            Self::Select(query) => Some(query.result_columns()),
            Self::Insert(insert) => Some(insert.result_columns()),
            Self::Update(update) => Some(update.result_columns()),
            Self::Delete(delete) => Some(delete.result_columns()),
        }
    }
    pub(crate) fn require_readable(&self) -> Result<(), AnalysisError> {
        if let Self::Update(update) = self
            && update.result_columns().is_empty()
        {
            return Err(AnalysisError::unexpected_input_error(
                "UPDATE CTE requires RETURNING to be read",
            ));
        }
        if let Self::Insert(insert) = self
            && insert.result_columns().is_empty()
        {
            return Err(AnalysisError::unexpected_input_error(
                "INSERT CTE requires RETURNING to be read",
            ));
        }
        if let Self::Delete(delete) = self
            && delete.result_columns().is_empty()
        {
            return Err(AnalysisError::unexpected_input_error(
                "DELETE CTE requires RETURNING to be read",
            ));
        }
        Ok(())
    }

    /// test column existing
    pub(crate) fn has_column(&self, column: &ColumnName) -> bool {
        match self {
            Self::Select(query) => query.has_column(column),
            Self::Insert(insert) => insert.result_columns().has_column(column),
            Self::Update(update) => update.result_columns().has_column(column),
            Self::Delete(delete) => delete.result_columns().has_column(column),
        }
    }

    pub(crate) fn from_delete(delete: &DeleteStatement) -> Self {
        Self::Delete(Box::new(delete.clone()))
    }
    pub(crate) fn from_update(update: UpdateStatement) -> Self {
        Self::Update(Box::new(update))
    }
    pub(crate) fn from_insert(insert: InsertStatement) -> Self {
        Self::Insert(Box::new(insert))
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
            Self::Insert(insert) => {
                let columns = insert.result_columns();
                let result = if let Some(index) = index {
                    columns.get_column_at(index)
                } else {
                    columns.get_column_with_skip_count(column, skip)
                };
                result.map(|column| column.create_column_definition())
            }
            Self::Update(update) => {
                let columns = update.result_columns();
                let result = if let Some(index) = index {
                    columns.get_column_at(index)
                } else {
                    columns.get_column_with_skip_count(column, skip)
                };
                result.map(|column| column.create_column_definition())
            }
            Self::Delete(delete) => {
                let columns = delete.result_columns();
                let result = if let Some(index) = index {
                    columns.get_column_at(index)
                } else {
                    columns.get_column_with_skip_count(column, skip)
                };
                result.map(|column| column.create_column_definition())
            }
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
        let result = syntax.resolve_with(&mut context);
        Ok((result?, context))
    }
}

impl CteStatement {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        match self {
            Self::Select(query) => query.walk(visitor)?,
            Self::Insert(insert) => insert.walk(visitor)?,
            Self::Update(update) => update.walk(visitor)?,
            Self::Delete(delete) => delete.walk(visitor)?,
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
