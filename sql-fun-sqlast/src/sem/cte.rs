use std::collections::HashMap;

use crate::{
    sem::{
        AnalysisError, AstAndContextPair, DeleteStatement, InsertStatement, ParseContext,
        SelectStatement, SemAst, UpdateStatement, analyze_delete_statement,
        analyze_insert_statement, analyze_select, analyze_update_statement,
        create_table::{ColumnDefinition, ColumnName},
    },
    syn::{ListOpt, Opt},
};

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
    fn analyze_syn<TParseContext>(
        context: TParseContext,
        syn: crate::syn::CommonTableExpr,
    ) -> Result<(CteStatement, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let query = syn.get_ctequery();

        if let Some(select) = query.as_select_stmt().as_inner() {
            let AstAndContextPair(sem, new_context) = analyze_select(context, select)?;
            let SemAst::SelectStatement(s) = sem else {
                AnalysisError::raise_unexpected_none("SemAst::SelectStatement")?
            };
            Ok((CteStatement::Select(Box::new(s)), new_context))
        } else if let Some(insert) = query.as_insert_stmt().as_inner() {
            let (sem, new_context) = analyze_insert_statement(context, &None, insert)?;
            let SemAst::InsertStatement(s) = sem else {
                AnalysisError::raise_unexpected_none("SemAst::InsertStatement")?
            };
            Ok((CteStatement::Insert(s), new_context))
        } else if let Some(update) = query.as_update_stmt().as_inner() {
            let (sem, new_context) = analyze_update_statement(context, &None, update)?;
            let SemAst::UpdateStatement(s) = sem else {
                AnalysisError::raise_unexpected_none("SemAst::UpdateStatement")?
            };
            Ok((CteStatement::Update(s), new_context))
        } else if let Some(delete) = query.as_delete_stmt().as_inner() {
            let (sem, new_context) = analyze_delete_statement(context, &None, delete)?;
            let SemAst::DeleteStatement(s) = sem else {
                AnalysisError::raise_unexpected_none("SemAst::DeleteStatement")?
            };
            Ok((CteStatement::Delete(s), new_context))
        } else {
            AnalysisError::raise_unexpected_none("common_table_expr.query")?
        }
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
    pub(crate) fn has_column(&self, _column: &ColumnName) -> bool {
        todo!()
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
    ) -> Result<(WithClause, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let mut with = WithClause::default();
        let Some(ctes) = syn.get_ctes().as_inner() else {
            return Ok((with, context));
        };

        for cte in ctes {
            let Some(table_expr) = cte.as_common_table_expr().as_inner() else {
                AnalysisError::raise_unexpected_none("cte.as_common_table_expr")?
            };
            let name = table_expr.get_ctename();
            let (cte, new_context) = CteStatement::analyze_syn(context, table_expr)?;
            with.insert(&name, cte);
            context = new_context;
        }
        Ok((with, context))
    }
}
