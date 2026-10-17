mod result_column;

pub use self::result_column::ResultColumnCollection;

use crate::{
    ast::TryFromWithTokens,
    sem::{
        AnalysisError, AstAndContextPair, CreateCompositeType, FromClause, ParseContext, SemAst,
        SemScalarExpr, TypeDefinition, TypeReference, WithClause,
        create_table::{ColumnDefinition, ColumnName},
    },
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

/// select statement
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SelectStatement {
    /// Query boundary with local CTEs and trailing clauses.
    Query(Box<crate::sem::SemanticQuery>),
    /// VALUES rows.
    Values(crate::sem::SemanticValues),
    /// simple query
    Simple(SelectStatementNode),
    /// union
    Union(Box<Self>, Box<Self>),
    /// intersect
    Intersect(Box<Self>, Box<Self>),
    /// except
    Except(Box<Self>, Box<Self>),
}

impl SelectStatement {
    pub(crate) fn has_width(&self, width: usize) -> bool {
        match self {
            Self::Simple(query) => query.result_columns.len() == width,
            Self::Query(query) => query.body().has_width(width),
            Self::Values(values) => values.rows().iter().all(|row| row.len() == width),
            Self::Union(left, right) | Self::Except(left, right) | Self::Intersect(left, right) => {
                left.has_width(width) && right.has_width(width)
            }
        }
    }
    pub(crate) fn rename_columns(&mut self, names: &[String]) {
        match self {
            Self::Simple(query) => query.result_columns.rename(names),
            Self::Query(query) => query.body_mut().rename_columns(names),
            Self::Values(values) => values.rename(names),
            Self::Union(left, _) | Self::Intersect(left, _) | Self::Except(left, _) => {
                left.rename_columns(names)
            }
        }
    }

    /// create a simple select query node
    #[must_use]
    pub fn simple(node: &SelectStatementNode) -> Self {
        Self::Simple(node.clone())
    }

    /// create a union node
    #[must_use]
    pub fn union(left: Self, right: Self) -> Self {
        Self::Union(Box::new(left), Box::new(right))
    }

    /// create a intersect node
    #[must_use]
    pub fn intersect(left: Self, right: Self) -> Self {
        Self::Intersect(Box::new(left), Box::new(right))
    }

    /// create except node
    #[must_use]
    pub fn except(left: Self, right: Self) -> Self {
        Self::Except(Box::new(left), Box::new(right))
    }

    /// get column definition by name
    #[must_use]
    pub fn get_result_column_def(&self, column: &ColumnName) -> Option<ColumnDefinition> {
        match self {
            Self::Simple(query) => query.get_result_column_def(column),
            Self::Query(query) => query.body().get_result_column_def(column),
            Self::Values(values) => values.columns().get_result_column_def(column),
            Self::Union(l, _r) => l.get_result_column_def(column),
            Self::Intersect(l, _r) => l.get_result_column_def(column),
            Self::Except(l, _r) => l.get_result_column_def(column),
        }
    }

    /// check column existing
    #[must_use]
    pub fn has_column(&self, column: &ColumnName) -> bool {
        match self {
            Self::Simple(query) => query.has_column(column),
            Self::Query(query) => query.body().has_column(column),
            Self::Values(values) => values.columns().has_column(column),
            Self::Union(l, _r) => l.has_column(column),
            Self::Intersect(l, _r) => l.has_column(column),
            Self::Except(l, _r) => l.has_column(column),
        }
    }

    /// enumerate result set columns
    #[must_use]
    pub fn result_columns(&self) -> &ResultColumnCollection {
        match self {
            Self::Simple(query) => &query.result_columns,
            Self::Query(query) => query.body().result_columns(),
            Self::Values(values) => values.columns(),
            Self::Union(l, _r) => l.result_columns(),
            Self::Intersect(l, _r) => l.result_columns(),
            Self::Except(l, _r) => l.result_columns(),
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
            Self::Union(l, _r) => l.returning_composite_type(),
            Self::Intersect(l, _r) => l.returning_composite_type(),
            Self::Except(l, _r) => l.returning_composite_type(),
        }
    }

    /// analyze [`crate::syn::SelectStmt`]
    pub fn analyze_select<TParseContext>(
        mut context: TParseContext,
        syn: crate::syn::SelectStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::Query::try_from_with_tokens(&syn, tokens)?;
        let outcome = syntax.resolve(&mut context);
        let query = outcome?;
        Ok(AstAndContextPair::new(SemAst::select(&query), context))
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
    clauses: Box<crate::sem::SelectClauses>,
}

impl SelectStatementNode {
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
        syn: crate::syn::SelectStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SelectStatement, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::Query::try_from_with_tokens(&syn, tokens)?;
        let outcome = syntax.resolve(&mut context);
        let query = outcome?;
        Ok((query, context))
    }
}

#[cfg(test)]
mod test_scalar_scope {
    use crate::{
        sem::{ColumnName, MutSourcePathWithSpan, PgBuiltInType, SourcePathWithSpan},
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
        let (select, context) = super::SelectStatementNode::analyze_select_node(
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
        let (select, context) = super::SelectStatementNode::analyze_select_node(
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
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        visitor.enter_query();
        let result = self.walk_body(visitor);
        visitor.leave_query();
        result?;
        Ok(())
    }
}

impl SelectStatement {
    pub(super) fn walk_body<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.walk_body_assigned(visitor, &[])
    }
    pub(crate) fn walk_assigned<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        targets: &[crate::sem::InsertTarget],
    ) -> Result<(), AnalysisError> {
        visitor.enter_query();
        let result = self.walk_body_assigned(visitor, targets);
        visitor.leave_query();
        result
    }
    pub(super) fn walk_body_assigned<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        targets: &[crate::sem::InsertTarget],
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Simple(query) => query.walk_assigned(visitor, targets),
            Self::Query(query) => query.walk_assigned(visitor, targets),
            Self::Values(values) => values.walk_assigned(visitor, targets),
            Self::Union(left, right) | Self::Intersect(left, right) | Self::Except(left, right) => {
                left.walk_assigned(visitor, targets)?;
                visitor.retain_set_outputs();
                right.walk_assigned(visitor, targets)
            }
        }
    }
}

impl SelectStatementNode {
    fn walk_assigned<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        targets: &[crate::sem::InsertTarget],
    ) -> Result<(), AnalysisError> {
        self.with_clause.walk(visitor)?;
        self.from_clause.walk(visitor)?;
        crate::sem::traversal::optional(&mut self.where_clause, visitor)?;
        if let Some(predicate) = &self.where_clause {
            visitor.predicate(predicate);
        }
        self.clauses.walk_windows(visitor)?;
        self.result_columns.walk_assigned(visitor, targets)?;
        self.clauses.walk(visitor)?;
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
        clauses: crate::sem::SelectClauses,
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
    pub(crate) fn query(query: crate::sem::SemanticQuery) -> Self {
        Self::Query(Box::new(query))
    }
    pub(crate) fn values(rows: Vec<Vec<SemScalarExpr>>) -> Self {
        Self::Values(crate::sem::SemanticValues::new(rows))
    }
    /// Query-wide clauses, if this value owns a query boundary.
    pub const fn query_clauses(&self) -> Option<&crate::sem::SemanticQuery> {
        match self {
            Self::Query(query) => Some(query),
            _ => None,
        }
    }
    /// SELECT-specific clauses, if this is a simple SELECT body.
    pub fn select_clauses(&self) -> Option<&crate::sem::SelectClauses> {
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
