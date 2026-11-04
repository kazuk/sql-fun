mod cursor;
mod destination;
mod row;
mod view_support;

pub use self::{
    cursor::DeleteCursor,
    row::{DeleteRowColumn, DeleteRowVersion},
    view_support::ViewDeleteSupport,
};
pub(crate) use self::{cursor::DeletePredicate, destination::DeleteDestination};
use crate::sem::convert;

use crate::{
    CacheContext,
    ast::{Delete, Dialect, StatementNode},
    sem::{AccessKind, traversal::SemanticVisitor},
    syn::DeleteStmt,
};

use crate::{
    ast::TryFromWithTokens,
    names::TableName,
    sem::{
        AnalysisError, FromClause, ParseContext, QueryCte, ResultColumnCollection, SemAst,
        SemScalarExpr, traversal::SemanticVisitorMut,
    },
};

/// An analyzed DELETE, including its destination, sources and RETURNING outputs.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeleteStatement {
    target: TableName,
    target_found: bool,
    with: Vec<QueryCte>,
    from: FromClause,
    predicate: Option<DeletePredicate>,
    returning: ResultColumnCollection,
    view_support: Option<ViewDeleteSupport>,
    inherit: bool,
    indexed: Option<String>,
    not_indexed: bool,
}
impl DeleteStatement {
    pub(crate) fn new(
        target: &TableName,
        target_found: bool,
        with: &[QueryCte],
        from: &FromClause,
        predicate: &Option<DeletePredicate>,
        returning: &ResultColumnCollection,
    ) -> Self {
        Self {
            target: target.clone(),
            target_found,
            with: with.to_vec(),
            from: from.clone(),
            predicate: predicate.clone(),
            returning: returning.clone(),
            view_support: None,
            inherit: false,
            indexed: None,
            not_indexed: false,
        }
    }
    pub(crate) fn set_modifiers(
        &mut self,
        support: Option<&ViewDeleteSupport>,
        indexed: &Option<String>,
        not_indexed: bool,
        inherit: bool,
    ) {
        self.view_support = support.cloned();
        self.indexed = indexed.clone();
        self.not_indexed = not_indexed;
        self.inherit = inherit;
    }
    /// Catalog destination identity, or the requested name when `target_found` is false.
    pub const fn target(&self) -> &TableName {
        &self.target
    }
    /// Whether destination lookup found a catalog relation. A false value retains
    /// the requested name in `target`, without claiming a qualified binding.
    pub const fn target_found(&self) -> bool {
        self.target_found
    }
    /// Local WITH definitions in declaration order.
    pub fn with(&self) -> &[QueryCte] {
        &self.with
    }
    /// Bound target and USING sources.
    pub const fn from(&self) -> &FromClause {
        &self.from
    }
    /// Resolved boolean WHERE expression; see `current_of` for positioned deletes.
    pub fn predicate(&self) -> Option<&SemScalarExpr> {
        if let Some(predicate) = &self.predicate {
            predicate.as_expression()
        } else {
            None
        }
    }
    /// Positioned WHERE predicate, including an unresolved portal reference.
    /// Mutually exclusive with the ordinary boolean predicate.
    pub const fn current_of(&self) -> Option<&DeleteCursor> {
        if let Some(predicate) = &self.predicate {
            predicate.as_cursor()
        } else {
            None
        }
    }
    /// RETURNING outputs; empty when RETURNING is absent.
    pub const fn result_columns(&self) -> &ResultColumnCollection {
        &self.returning
    }
    /// Catalog decision for an ordinary view destination.
    pub const fn view_support(&self) -> Option<&ViewDeleteSupport> {
        self.view_support.as_ref()
    }
    /// Whether PostgreSQL descendants are included (false for ONLY).
    pub const fn includes_descendants(&self) -> bool {
        self.inherit
    }
    /// Explicit SQLite INDEXED BY name.
    pub fn indexed(&self) -> Option<&str> {
        self.indexed.as_deref()
    }
    /// Whether SQLite NOT INDEXED was specified.
    pub const fn not_indexed(&self) -> bool {
        self.not_indexed
    }
    pub(crate) fn rename_columns(&mut self, names: &[String]) {
        self.returning.rename(names);
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
        visitor.table_binding(&mut self.target);
        for cte in &mut self.with {
            cte.walk(visitor)?;
        }
        self.from.walk(visitor)?;
        if let Some(predicate) = &mut self.predicate {
            predicate.walk(visitor)?;
        }
        visitor.dml_returning(&mut self.returning)?;
        Ok(())
    }
    fn visit_body<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.access_action(self.target.full_name(), AccessKind::delete_rows());
        visitor.access_purpose(AccessKind::delete_source());
        visitor.table_binding(&self.target);
        for cte in &self.with {
            cte.visit(visitor)?;
        }
        self.from.visit(visitor)?;
        visitor.access_purpose(AccessKind::delete_filter());
        if let Some(predicate) = &self.predicate {
            predicate.visit(visitor)?;
        }
        visitor.access_purpose(AccessKind::delete_returning());
        visitor.dml_returning(&self.returning)?;
        Ok(())
    }
}

/// Analyze PostgreSQL DELETE through the common AST and ParseContext.
///
/// # Errors
/// Returns an error when syntax conversion or semantic analysis fails.
pub fn analyze_delete_statement<T: ParseContext>(
    mut context: T,
    _parent_schema: &Option<String>,
    syn: DeleteStmt,
) -> Result<(SemAst, T), AnalysisError> {
    AnalysisError::check_dialect(Dialect::postgres(), context.dialect())?;
    let syntax = Delete::try_from_with_tokens(&syn, &::sql_fun_core::IVec::default())?;
    let statement =
        convert(&StatementNode::Delete(syntax)).resolve(&mut context, &mut CacheContext::new())?;
    Ok((statement, context))
}
