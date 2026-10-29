use crate::{
    ast::TryFromWithTokens,
    context::ParseContext,
    sem::{
        AnalysisError, DeleteStatement, FullName, InsertStatement, MergeStatement,
        ResultColumnCollection, SelectStatement, UpdateStatement,
        create_table::{ColumnDefinition, ColumnName},
        traversal::SemanticVisitor,
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
    /// A diagnosed CTE body that could not be resolved.
    Unresolved,
    /// `SELECT` statement
    Select(Box<SelectStatement>),
    /// `INSERT` statement
    Insert(Box<InsertStatement>),
    /// `UPDATE` statement
    Update(Box<UpdateStatement>),
    /// `DELETE` statement
    Delete(Box<DeleteStatement>),
    /// `MERGE` statement
    Merge(Box<MergeStatement>),
}

impl CteStatement {
    pub(crate) const fn unresolved() -> Self {
        Self::Unresolved
    }

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

    pub(crate) fn result_columns(&self) -> Option<&ResultColumnCollection> {
        match self {
            Self::Unresolved => None,
            Self::Select(query) => Some(query.result_columns()),
            Self::Insert(insert) => Some(insert.result_columns()),
            Self::Update(update) => Some(update.result_columns()),
            Self::Delete(delete) => Some(delete.result_columns()),
            Self::Merge(merge) => Some(merge.result_columns()),
        }
    }
    pub(crate) fn is_unreadable_delete(&self) -> bool {
        matches!(self, Self::Delete(delete) if delete.result_columns().is_empty())
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
        if let Self::Merge(merge) = self
            && merge.result_columns().is_empty()
        {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE CTE requires RETURNING to be read",
            ));
        }
        Ok(())
    }

    /// test column existing
    pub(crate) fn has_column(&self, column: &ColumnName) -> bool {
        match self {
            Self::Unresolved => false,
            Self::Select(query) => query.has_column(column),
            Self::Insert(insert) => insert.result_columns().has_column(column),
            Self::Update(update) => update.result_columns().has_column(column),
            Self::Delete(delete) => delete.result_columns().has_column(column),
            Self::Merge(merge) => merge.result_columns().has_column(column),
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

    pub(crate) fn from_merge(merge: MergeStatement) -> Self {
        Self::Merge(Box::new(merge))
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
            Self::Unresolved => None,
            Self::Select(query) => SelectStatement::resolved_column(column, index, skip, query),
            Self::Insert(insert) => InsertStatement::resolved_column(column, index, skip, insert),
            Self::Update(update) => UpdateStatement::resolved_column(column, index, skip, update),
            Self::Merge(merge) => MergeStatement::resolved_column(column, index, skip, merge),
            Self::Delete(delete) => DeleteStatement::resolved_column(column, index, skip, delete),
        }
    }
}

impl DeleteStatement {
    fn resolved_column(
        column: &ColumnName,
        index: Option<usize>,
        skip: usize,
        delete: &Self,
    ) -> Option<ColumnDefinition> {
        let columns = delete.result_columns();
        let result = if let Some(index) = index {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, skip)
        };
        result.map(|column| column.create_column_definition())
    }
}

impl MergeStatement {
    fn resolved_column(
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

impl UpdateStatement {
    fn resolved_column(
        column: &ColumnName,
        index: Option<usize>,
        skip: usize,
        update: &Self,
    ) -> Option<ColumnDefinition> {
        let columns = update.result_columns();
        let result = if let Some(index) = index {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, skip)
        };
        result.map(|column| column.create_column_definition())
    }
}

impl InsertStatement {
    fn resolved_column(
        column: &ColumnName,
        index: Option<usize>,
        skip: usize,
        insert: &Self,
    ) -> Option<ColumnDefinition> {
        let columns = insert.result_columns();
        let result = if let Some(index) = index {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, skip)
        };
        result.map(|column| column.create_column_definition())
    }
}

impl SelectStatement {
    fn resolved_column(
        column: &ColumnName,
        index: Option<usize>,
        skip: usize,
        query: &Self,
    ) -> Option<ColumnDefinition> {
        let columns = query.result_columns();
        let result = if let Some(index) = index {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, skip)
        };
        result.map(|column| column.create_column_definition())
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
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(crate) fn resolve_name(
        &self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Option<&CteName> {
        if name.schema().is_some() || name.catalog().is_some() {
            return None;
        }
        self.0.keys().find(|candidate| {
            if context.dialect() == crate::ast::Dialect::sqlite() {
                candidate.eq_ignore_ascii_case(name.name().as_str())
            } else {
                candidate.as_str() == name.name().as_str()
            }
        })
    }
    pub(crate) fn insert_resolved(
        &mut self,
        context: &impl ParseContext,
        name: &CteName,
        statement: CteStatement,
    ) {
        if context.dialect() == crate::ast::Dialect::sqlite() {
            self.0
                .retain(|candidate, _| !candidate.eq_ignore_ascii_case(name));
        }
        self.insert(name, statement);
    }
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
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Unresolved => {}
            Self::Select(query) => query.walk(visitor)?,
            Self::Insert(insert) => insert.walk(visitor)?,
            Self::Update(update) => update.walk(visitor)?,
            Self::Delete(delete) => delete.walk(visitor)?,
            Self::Merge(merge) => merge.walk(visitor)?,
        }
        Ok(())
    }
}

impl WithClause {
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for (name, query) in &mut self.0 {
            if let Some(columns) = query.result_columns() {
                visitor.source_outputs(&columns.names());
            }
            query.walk(visitor)?;
            visitor.cte(name);
            if let Some(columns) = query.result_columns() {
                visitor.cte_columns(name, &columns.names());
            }
        }
        Ok(())
    }
}
