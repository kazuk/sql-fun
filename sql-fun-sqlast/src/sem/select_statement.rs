mod result_column;
mod set_operation;

pub(crate) use self::result_column::ResultColumn;
pub use self::result_column::ResultColumnCollection;
pub use self::set_operation::{SetOperationKind, SetOperationQuery};

use crate::{
    CacheContext,
    ast::Query as AstQuery,
    sem::{
        AccessKind, InsertTarget, SelectClauses, SemanticQuery, SemanticValues,
        traversal::{SemanticVisitor, SemanticVisitorMut, optional, visit_optional},
    },
    syn::SelectStmt,
};

use crate::{
    ast::TryFromWithTokens,
    names::ColumnName,
    sem::{
        AnalysisError, AstAndContextPair, CreateCompositeType, FromClause, ParseContext, SemAst,
        SemScalarExpr, TypeDefinition, TypeReference, WithClause, create_table::ColumnDefinition,
    },
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

/// select statement
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SelectStatement {
    /// Query boundary with local CTEs and trailing clauses.
    Query(Box<SemanticQuery>),
    /// VALUES rows.
    Values(SemanticValues),
    /// simple query
    Simple(SelectStatementNode),
    /// Binary set operation with analyzed output semantics.
    Set(Box<SetOperationQuery>),
}

impl SelectStatement {
    pub(crate) fn set_operations_valid(&self) -> Result<bool, AnalysisError> {
        let mut validity = SetOperationValidity(true);
        self.clone().walk(&mut validity)?;
        Ok(validity.0)
    }

    pub(crate) fn has_pending_stars(&self) -> bool {
        match self {
            Self::Simple(query) => query.result_columns.has_pending_stars(),
            Self::Query(query) => query.body().has_pending_stars(),
            Self::Set(query) => query.has_pending_stars(),
            Self::Values(_) => false,
        }
    }

    pub(crate) fn expand_pending_stars(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match self {
            Self::Simple(query) => query.result_columns.expand_pending_stars(context),
            Self::Query(query) => query.body_mut().expand_pending_stars(context),
            Self::Set(query) => query.expand_pending_stars(context),
            Self::Values(_) => Ok(false),
        }
    }

    pub(crate) fn assign_update_output<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        index: usize,
        ty: &TypeReference,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Simple(query) => query
                .result_columns
                .assign_update_output(visitor, index, ty),
            Self::Query(query) => query.body_mut().assign_update_output(visitor, index, ty),
            Self::Values(values) => values.assign_update_output(visitor, index, ty),
            Self::Set(query) => query.assign_update_output(visitor, index, ty),
        }
    }

    pub(crate) fn has_width(&self, width: usize) -> bool {
        match self {
            Self::Simple(query) => query.result_columns.len() == width,
            Self::Query(query) => query.body().has_width(width),
            Self::Values(values) => values.rows().iter().all(|row| row.len() == width),
            Self::Set(query) => query.has_width(width),
        }
    }
    pub(crate) fn rename_columns(&mut self, names: &[String]) {
        match self {
            Self::Simple(query) => query.result_columns.rename(names),
            Self::Query(query) => query.body_mut().rename_columns(names),
            Self::Values(values) => values.rename(names),
            Self::Set(query) => query.rename_columns(names),
        }
    }

    /// create a simple select query node
    #[must_use]
    pub fn simple(node: &SelectStatementNode) -> Self {
        Self::Simple(node.clone())
    }

    /// Analyze a binary set operation in the selected dialect.
    pub fn set_operation(
        context: &mut impl ParseContext,
        kind: SetOperationKind,
        left: Self,
        right: Self,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Set(Box::new(SetOperationQuery::resolve(
            context, kind, left, right,
        )?)))
    }

    /// get column definition by name
    #[must_use]
    pub fn get_result_column_def(&self, column: &ColumnName) -> Option<ColumnDefinition> {
        match self {
            Self::Simple(query) => query.get_result_column_def(column),
            Self::Query(query) => query.body().get_result_column_def(column),
            Self::Values(values) => values.columns().get_result_column_def(column),
            Self::Set(query) => query.columns().get_result_column_def(column),
        }
    }

    /// check column existing
    #[must_use]
    pub fn has_column(&self, column: &ColumnName) -> bool {
        match self {
            Self::Simple(query) => query.has_column(column),
            Self::Query(query) => query.body().has_column(column),
            Self::Values(values) => values.columns().has_column(column),
            Self::Set(query) => query.columns().has_column(column),
        }
    }

    /// enumerate result set columns
    #[must_use]
    pub fn result_columns(&self) -> &ResultColumnCollection {
        match self {
            Self::Simple(query) => &query.result_columns,
            Self::Query(query) => query.body().result_columns(),
            Self::Values(values) => values.columns(),
            Self::Set(query) => query.columns(),
        }
    }

    /// returning columns as composite type
    pub fn returning_composite_type(&self) -> Result<TypeReference, AnalysisError> {
        match self {
            Self::Simple(query) => query.returning_composite_type(),
            Self::Query(query) => query.body().returning_composite_type(),
            Self::Values(values) => {
                let fields = values.columns().composite_field_collection()?;
                Ok(TypeReference::dynamic(&TypeDefinition::CompositeType(
                    CreateCompositeType::from_fields(&fields),
                )))
            }
            Self::Set(query) => {
                let fields = query.columns().composite_field_collection()?;
                Ok(TypeReference::dynamic(&TypeDefinition::CompositeType(
                    CreateCompositeType::from_fields(&fields),
                )))
            }
        }
    }

    /// analyze [`crate::syn::SelectStmt`]
    pub fn analyze_select<TParseContext>(
        mut context: TParseContext,
        syn: SelectStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = AstQuery::try_from_with_tokens(&syn, tokens)?;
        let outcome = syntax.resolve(&mut context, &mut CacheContext::new());
        let query = outcome?;
        Ok(AstAndContextPair::new(SemAst::select(&query), context))
    }
}

struct SetOperationValidity(bool);
impl crate::sem::traversal::SemanticVisitorMut for SetOperationValidity {
    fn walk_source_column_values(&self) -> bool {
        false
    }

    fn set_operation(&mut self, query: &mut SetOperationQuery) -> Result<(), AnalysisError> {
        self.0 &= query.can_apply();
        Ok(())
    }
}

/// `select` statement semantic AST
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectStatementNode {
    result_columns: ResultColumnCollection,
    with_clause: WithClause,
    from_clause: FromClause,
    where_clause: Option<SemScalarExpr>,
    // Keep clause payloads indirect: SelectStatement also determines the size
    // of every SemAst retained while loading a schema dump.
    #[serde(default)]
    clauses: Box<SelectClauses>,
}

impl SelectStatementNode {
    pub(crate) const fn with_clause(&self) -> &WithClause {
        &self.with_clause
    }
    pub fn new(
        result_columns: &ResultColumnCollection,
        with_clause: &WithClause,
        from_clause: &FromClause,
        where_clause: &Option<SemScalarExpr>,
    ) -> Self {
        Self {
            result_columns: result_columns.clone(),
            with_clause: with_clause.clone(),
            from_clause: from_clause.clone(),
            where_clause: where_clause.clone(),
            clauses: Box::default(),
        }
    }
    /// enumerate result set columns
    pub const fn result_columns(&self) -> &ResultColumnCollection {
        &self.result_columns
    }

    /// get result set column definition
    pub fn get_result_column_def(&self, column_name: &ColumnName) -> Option<ColumnDefinition> {
        self.result_columns.get_result_column_def(column_name)
    }

    /// check a column in result set
    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.result_columns.has_column(column)
    }

    fn returning_composite_type(&self) -> Result<TypeReference, AnalysisError> {
        let fields = self.result_columns.composite_field_collection()?;
        let cct = CreateCompositeType::from_fields(&fields);
        let type_def = TypeDefinition::CompositeType(cct);
        Ok(TypeReference::dynamic(&type_def))
    }

    /// analyze [`crate::syn::SelectStmt`]
    pub fn analyze_select_node<TParseContext>(
        mut context: TParseContext,
        syn: SelectStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SelectStatement, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = AstQuery::try_from_with_tokens(&syn, tokens)?;
        let outcome = syntax.resolve(&mut context, &mut CacheContext::new());
        let query = outcome?;
        Ok((query, context))
    }
}

#[cfg(test)]
mod test_scalar_scope {
    use crate::sem::select_statement::SelectStatementNode;

    use crate::{
        context::MutSourcePathWithSpan,
        names::ColumnName,
        sem::{PgBuiltInType, SourcePathWithSpan},
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext, source_span},
    };

    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    fn source_columns_remain_visible_in_nested_scalar_expressions(
        source_span: SourcePathWithSpan,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.set_source_path_with_span(&source_span);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::bool());
        let syn = SynBuilder::new().parse(
            "select case when true then coalesce(s.n) else n end as result \
             from (select 42 as n) s where s.n is not null",
        );
        let (select, context) = SelectStatementNode::analyze_select_node(
            context,
            syn.as_select_stmt().as_inner().expect("select statement"),
            &IVec::default(),
        )?;
        let column = select
            .get_result_column_def(&ColumnName::from("result"))
            .expect("result column");
        assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }

    #[rstest::rstest]
    #[case("n")]
    #[case("missing")]
    #[case("unknown.n")]
    #[case("a.missing")]
    fn unresolved_column_reports_a_problem(
        source_span: SourcePathWithSpan,
        #[case] expression: &str,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.set_source_path_with_span(&source_span);
        let syn = SynBuilder::new().parse(&format!(
            "select {expression} as result from (select 42 as n) a, (select 43 as n) b"
        ));
        let (select, context) = SelectStatementNode::analyze_select_node(
            context,
            syn.as_select_stmt().as_inner().expect("select statement"),
            &IVec::default(),
        )?;
        let column = select
            .get_result_column_def(&ColumnName::from("result"))
            .expect("result column");
        assert_eq!(column.get_type_cloned(), None);
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }
}

impl SelectStatement {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.enter_query();
        let result = self.walk_body(visitor);
        visitor.leave_query();
        result?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.enter_query();
        let result = self.visit_body(visitor);
        visitor.leave_query();
        result?;
        Ok(())
    }
}

impl SelectStatement {
    pub(super) fn walk_body<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.walk_body_assigned(visitor, &[])
    }
    pub(super) fn visit_body<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.visit_body_assigned(visitor, &[])
    }
    pub(crate) fn walk_assigned<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        visitor.enter_query();
        let result = self.walk_body_assigned(visitor, targets);
        visitor.leave_query();
        result
    }
    pub(crate) fn visit_assigned<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        visitor.enter_query();
        let result = self.visit_body_assigned(visitor, targets);
        visitor.leave_query();
        result
    }
    pub(super) fn walk_body_assigned<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Simple(query) => query.walk_assigned(visitor, targets),
            Self::Query(query) => query.walk_assigned(visitor, targets),
            Self::Values(values) => values.walk_assigned(visitor, targets),
            Self::Set(query) => query.walk_assigned(visitor, targets),
        }
    }
    pub(super) fn visit_body_assigned<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Simple(query) => query.visit_assigned(visitor, targets),
            Self::Query(query) => query.visit_assigned(visitor, targets),
            Self::Values(values) => values.visit_assigned(visitor, targets),
            Self::Set(query) => query.visit_assigned(visitor, targets),
        }
    }
}

impl SelectStatementNode {
    fn walk_assigned<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        self.with_clause.walk(visitor)?;
        self.from_clause.walk(visitor)?;
        optional(&mut self.where_clause, visitor)?;
        if let Some(predicate) = &self.where_clause {
            visitor.predicate(predicate);
        }
        self.clauses.walk_windows(visitor)?;
        self.result_columns.walk_assigned(visitor, targets)?;
        self.clauses.walk(visitor)?;
        Ok(())
    }
    fn visit_assigned<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        visitor.access_purpose(AccessKind::select_source());
        self.with_clause.visit(visitor)?;
        self.from_clause.visit(visitor)?;
        visitor.access_purpose(AccessKind::select_filter());
        visit_optional(&self.where_clause, visitor)?;
        if let Some(predicate) = &self.where_clause {
            visitor.predicate(predicate);
        }
        self.clauses.visit_windows(visitor)?;
        visitor.access_purpose(AccessKind::select_output());
        self.result_columns.visit_assigned(visitor, targets)?;
        self.clauses.visit(visitor)?;
        Ok(())
    }
}

impl From<SelectStatementNode> for SelectStatement {
    fn from(node: SelectStatementNode) -> Self {
        Self::Simple(node)
    }
}
impl SelectStatementNode {
    pub(crate) fn from_resolution(
        result_columns: ResultColumnCollection,
        with_clause: &WithClause,
        from_clause: FromClause,
        where_clause: Option<SemScalarExpr>,
        clauses: SelectClauses,
    ) -> Self {
        Self {
            result_columns,
            with_clause: with_clause.clone(),
            from_clause,
            where_clause,
            clauses: Box::new(clauses),
        }
    }
}

impl SelectStatement {
    pub(crate) fn query(query: SemanticQuery) -> Self {
        Self::Query(Box::new(query))
    }
    pub(crate) fn values(rows: Vec<Vec<SemScalarExpr>>) -> Self {
        Self::Values(SemanticValues::new(rows))
    }
    /// Query-wide clauses, if this value owns a query boundary.
    pub const fn query_clauses(&self) -> Option<&SemanticQuery> {
        match self {
            Self::Query(query) => Some(query),
            _ => None,
        }
    }
    /// SELECT-specific clauses, if this is a simple SELECT body.
    pub fn select_clauses(&self) -> Option<&SelectClauses> {
        match self {
            Self::Simple(node) => Some(&node.clauses),
            _ => None,
        }
    }
    pub(crate) const fn source_scope(&self) -> Option<&FromClause> {
        match self {
            Self::Simple(node) => Some(&node.from_clause),
            _ => None,
        }
    }
}
