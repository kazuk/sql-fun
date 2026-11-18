mod ast;
use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{ColumnName, CteName, FullName},
    sem::{
        AnalysisError, DeleteStatement, InsertStatement, MergeStatement, QueryCte,
        ResultColumnCollection, SelectStatement, UpdateStatement,
        create_table::ColumnDefinition,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

use std::collections::HashMap;

/// CTE query item types
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CteStatement {
    /// A diagnosed CTE body that could not be resolved.
    Unresolved,
    /// Analyzed body of a CTE with invalid recursion.
    InvalidRecursive(Box<Self>),
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
            Self::InvalidRecursive(body) => body.result_columns(),
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
            Self::InvalidRecursive(body) => body.has_column(column),
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
            Self::InvalidRecursive(body) => body.resolved_column(column, index, skip),
            Self::Select(query) => SelectStatement::resolved_column(column, index, skip, query),
            Self::Insert(insert) => InsertStatement::resolved_column(column, index, skip, insert),
            Self::Update(update) => UpdateStatement::resolved_column(column, index, skip, update),
            Self::Merge(merge) => MergeStatement::resolved_column(column, index, skip, merge),
            Self::Delete(delete) => DeleteStatement::resolved_column(column, index, skip, delete),
        }
    }
}

/// Analyzed WITH clause.
#[derive(Debug, Clone, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct WithClause {
    definitions: HashMap<CteName, CteStatement>,
    #[serde(skip)]
    resolution_order: Vec<CteName>,
    #[serde(skip)]
    recursive_anchors: Vec<CteName>,
    #[serde(skip)]
    rejected_definitions: Vec<QueryCte>,
}

impl std::hash::Hash for WithClause {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (k, v) in &self.definitions {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl WithClause {
    pub(crate) fn with_recursive_anchor(
        &self,
        context: &impl ParseContext,
        name: &CteName,
        binding: CteStatement,
    ) -> Self {
        let mut scope = self.clone();
        scope.insert_resolved(context, name, binding);
        scope.recursive_anchors.push(name.clone());
        scope
    }

    pub(crate) fn is_recursive_anchor(&self, name: &CteName) -> bool {
        self.recursive_anchors.contains(name)
    }

    pub(crate) fn record_resolution(&mut self, name: &CteName) {
        self.resolution_order.push(name.clone());
    }

    pub(crate) fn resolution_order(&self, name: &str) -> Option<usize> {
        self.resolution_order
            .iter()
            .position(|candidate| candidate.as_str() == name)
    }

    pub(crate) fn clear_resolution_order(&mut self) {
        self.resolution_order.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
    pub(crate) fn resolve_name(
        &self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Option<&CteName> {
        if name.schema().is_some() || name.catalog().is_some() {
            return None;
        }
        self.definitions.keys().find(|candidate| {
            if context.dialect() == Dialect::sqlite() {
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
        if context.dialect() == Dialect::sqlite() {
            self.definitions
                .retain(|candidate, _| !candidate.eq_ignore_ascii_case(name));
            self.recursive_anchors
                .retain(|candidate| !candidate.eq_ignore_ascii_case(name));
        } else {
            self.recursive_anchors.retain(|candidate| candidate != name);
        }
        self.insert(name, statement);
    }
    pub(crate) fn take(&mut self, name: &str) -> Option<CteStatement> {
        self.definitions.remove(name)
    }
    /// get CTE query by name
    #[must_use]
    pub fn get(&self, name: &CteName) -> Option<&CteStatement> {
        self.definitions.get(name)
    }

    /// insert named CTE item
    pub fn insert(&mut self, name: &CteName, statement: CteStatement) {
        self.definitions.insert(name.clone(), statement);
    }
}

impl CteStatement {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Unresolved => {}
            Self::InvalidRecursive(body) => {
                visitor.invalid_recursive_cte();
                body.walk(visitor)?;
            }
            Self::Select(query) => query.walk(visitor)?,
            Self::Insert(insert) => insert.walk(visitor)?,
            Self::Update(update) => update.walk(visitor)?,
            Self::Delete(delete) => delete.walk(visitor)?,
            Self::Merge(merge) => merge.walk(visitor)?,
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Unresolved => {}
            Self::InvalidRecursive(body) => body.visit(visitor)?,
            Self::Select(query) => query.visit(visitor)?,
            Self::Insert(insert) => insert.visit(visitor)?,
            Self::Update(update) => update.visit(visitor)?,
            Self::Delete(delete) => delete.visit(visitor)?,
            Self::Merge(merge) => merge.visit(visitor)?,
        }
        Ok(())
    }
}

impl WithClause {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for (name, query) in &mut self.definitions {
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
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for (name, query) in &self.definitions {
            if let Some(columns) = query.result_columns() {
                visitor.source_outputs(&columns.names());
            }
            query.visit(visitor)?;
            visitor.cte(name);
            if let Some(columns) = query.result_columns() {
                visitor.cte_columns(name, &columns.names());
            }
        }
        Ok(())
    }
}
