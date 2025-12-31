use crate::{
    sem::{
        AnalysisError, AstAndContextPair, CreateCompositType, FromClause, ParseContext, SemAst,
        SemScalarExpr, TypeDefinition, TypeReference, WithClause, analyze_scaler_expr,
        create_table::{ColumnDefinition, ColumnName},
    },
    syn::{ListOpt, Opt, SetOperation},
};

mod result_column;
pub use self::result_column::ResultColumnCollection;

/// select statement
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SelectStatement {
    /// simple query
    Simple(SelectStatementNode),
    /// union
    Union(Box<SelectStatement>, Box<SelectStatement>),
    /// intersect
    Intersect(Box<SelectStatement>, Box<SelectStatement>),
    /// except
    Except(Box<SelectStatement>, Box<SelectStatement>),
}

impl SelectStatement {
    /// create a simple select query node
    pub fn simple(node: &SelectStatementNode) -> Self {
        Self::Simple(node.clone())
    }

    /// create a union node
    pub fn union(left: SelectStatement, right: SelectStatement) -> Self {
        Self::Union(Box::new(left), Box::new(right))
    }

    /// create a intersect node
    pub fn intersect(left: SelectStatement, right: SelectStatement) -> Self {
        Self::Intersect(Box::new(left), Box::new(right))
    }

    /// create except node
    pub fn except(left: SelectStatement, right: SelectStatement) -> Self {
        Self::Except(Box::new(left), Box::new(right))
    }

    /// get column definition by name
    pub fn get_result_column_def(&self, column: &ColumnName) -> Option<ColumnDefinition> {
        match self {
            Self::Simple(query) => query.get_result_column_def(column),
            Self::Union(l, _r) => l.get_result_column_def(column),
            Self::Intersect(l, _r) => l.get_result_column_def(column),
            Self::Except(l, _r) => l.get_result_column_def(column),
        }
    }

    /// check column existing
    pub fn has_column(&self, column: &ColumnName) -> bool {
        match self {
            Self::Simple(query) => query.has_column(column),
            Self::Union(l, _r) => l.has_column(column),
            Self::Intersect(l, _r) => l.has_column(column),
            Self::Except(l, _r) => l.has_column(column),
        }
    }

    /// enumerate result set columns
    pub fn result_columns(&self) -> &ResultColumnCollection {
        match self {
            Self::Simple(query) => &query.result_columns,
            Self::Union(l, _r) => &l.result_columns(),
            Self::Intersect(l, _r) => &l.result_columns(),
            Self::Except(l, _r) => &l.result_columns(),
        }
    }

    /// returning columns as composite type
    pub fn returning_composite_type(&self) -> Result<TypeReference, AnalysisError> {
        match self {
            Self::Simple(query) => query.returning_composite_type(),
            Self::Union(l, _r) => l.returning_composite_type(),
            Self::Intersect(l, _r) => l.returning_composite_type(),
            Self::Except(l, _r) => l.returning_composite_type(),
        }
    }
}

/// `select` statement semantic AST
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectStatementNode {
    result_columns: ResultColumnCollection,
    with_clause: WithClause,
    from_clause: FromClause,
    where_clause: Option<SemScalarExpr>,
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
        }
    }
    /// enumerate result set columns
    pub fn result_columns(&self) -> &ResultColumnCollection {
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
        let cct = CreateCompositType::from_fields(&fields);
        let type_def = TypeDefinition::CompositType(cct);
        Ok(TypeReference::dynamic(&type_def))
    }
}

/// analyze [crate::syn::SelectStmt]
pub fn analyze_select<TParseContext>(
    context: TParseContext,
    syn: crate::syn::SelectStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(set_operation) = syn.get_op().as_inner() else {
        AnalysisError::raise_unexpected_none("selectstmt.op")?
    };

    match set_operation {
        SetOperation::SetopNone => {
            let (node, context) = analyze_select_node(context, syn)?;
            Ok(AstAndContextPair::new(
                SemAst::SelectStatement(node),
                context,
            ))
        }
        SetOperation::SetopUnion => analyze_select_union(context, &syn),
        SetOperation::SetopIntersect => analyze_select_intersect(context, &syn),
        SetOperation::SetopExcept => analyze_select_except(context, &syn),
        _ => AnalysisError::raise_unexpected_input("selectstmt.op")?,
    }
}

fn analyze_setop_select<TParseContext>(
    context: TParseContext,
    syn: &crate::syn::SelectStmt,
) -> Result<(SelectStatement, SelectStatement, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(larg) = syn.get_larg().as_inner() else {
        AnalysisError::raise_unexpected_none("selectstmt.larg")?
    };
    let Some(rarg) = syn.get_rarg().as_inner() else {
        AnalysisError::raise_unexpected_none("selectstmt.rarg")?
    };
    let (larg, context) = analyze_select_node(context, larg)?;
    let (rarg, context) = analyze_select_node(context, rarg)?;
    Ok((larg, rarg, context))
}

fn analyze_select_except<TParseContext>(
    context: TParseContext,
    syn: &crate::syn::SelectStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let (larg, rarg, context) = analyze_setop_select(context, syn)?;
    Ok(AstAndContextPair::new(
        SemAst::SelectStatement(SelectStatement::except(larg, rarg)),
        context,
    ))
}

fn analyze_select_intersect<TParseContext>(
    context: TParseContext,
    syn: &crate::syn::SelectStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let (larg, rarg, context) = analyze_setop_select(context, syn)?;
    Ok(AstAndContextPair::new(
        SemAst::SelectStatement(SelectStatement::intersect(larg, rarg)),
        context,
    ))
}

fn analyze_select_union<TParseContext>(
    context: TParseContext,
    syn: &crate::syn::SelectStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let (larg, rarg, context) = analyze_setop_select(context, syn)?;
    Ok(AstAndContextPair::new(
        SemAst::SelectStatement(SelectStatement::union(larg, rarg)),
        context,
    ))
}

/// analyze [crate::syn::SelectStmt]
pub fn analyze_select_node<TParseContext>(
    mut context: TParseContext,
    syn: crate::syn::SelectStmt,
) -> Result<(SelectStatement, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    let (with_clause, new_context) = WithClause::analyze(context, syn.get_with_clause())?;
    context = new_context;
    let (from_clause, new_context) =
        FromClause::analyze(context, &with_clause, syn.get_from_clause())?;
    context = new_context;

    let where_clause = if let Some(where_clause) = syn.get_where_clause().as_inner() {
        let (where_expr, new_context) =
            analyze_scaler_expr(context, &with_clause, &from_clause, where_clause)?;
        context = new_context;
        Some(where_expr)
    } else {
        None
    };

    let Some(target_list) = syn.get_target_list().map(|n| n.as_res_target()) else {
        AnalysisError::raise_unexpected_none("select_stmt.target_list")?
    };
    let mut result_columns = ResultColumnCollection::default();
    for res_target in target_list {
        let Some(val) = res_target.get_val().as_inner() else {
            AnalysisError::raise_unexpected_none("restarget.val")?
        };
        let (val_sem, new_context) = analyze_scaler_expr(context, &with_clause, &from_clause, val)?;
        context = new_context;

        let mut name = res_target.get_name();
        if name.is_empty() {
            name = val_sem.get_column_name();
        }
        let name = ColumnName::new(&name);
        result_columns.push_column(&name, &val_sem);
    }

    let select =
        SelectStatementNode::new(&result_columns, &with_clause, &from_clause, &where_clause);
    Ok((SelectStatement::simple(&select), context))
}
