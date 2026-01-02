use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AstAndContextPair, ColumnName, FromClause, ParseContext, SemAst,
        SemScalarExpr, TableName, WithClause, analyze_scaler_expr,
    },
    syn::{ListOpt, Opt, SortByDir},
};

/// Index item sort order
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    /// assending
    Asc,
    /// dessending
    Desc,
}

/// `IndexElement` has column or expression
#[derive(Debug, Clone)]
pub enum IndexElement {
    /// column valiant
    ColumnName(ColumnName),
    /// expression variant
    Expr(Box<SemScalarExpr>),
}

impl From<&ColumnName> for IndexElement {
    fn from(value: &ColumnName) -> Self {
        Self::ColumnName(value.clone())
    }
}

impl From<&SemScalarExpr> for IndexElement {
    fn from(value: &SemScalarExpr) -> Self {
        Self::Expr(Box::new(value.clone()))
    }
}

/// combine column and sort order , `col_name`
#[expect(dead_code)]
#[derive(Debug, Clone)]
pub struct IndexElementWithOrder {
    element: IndexElement,
    col_name: Option<String>,
    order: SortOrder,
}

impl IndexElementWithOrder {
    /// create instance
    #[must_use]
    pub fn new(element: &IndexElement, order: SortOrder, col_name: &Option<String>) -> Self {
        Self {
            element: element.clone(),
            col_name: col_name.clone(),
            order,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct IndexElementCollection(Vec<IndexElementWithOrder>);

impl IndexElementCollection {
    pub fn from_column_names(column_names: &IVec<ColumnName>) -> Self {
        Self(
            column_names
                .iter()
                .map(|v| {
                    let col = IndexElement::from(v);
                    IndexElementWithOrder::new(&col, SortOrder::Asc, &None)
                })
                .collect(),
        )
    }

    fn push(&mut self, item: IndexElementWithOrder) {
        self.0.push(item);
    }
}

/// analyzed [`crate::syn::IndexStmt`]
#[expect(dead_code)]
#[derive(Debug, Clone)]
pub struct CreateIndex {
    name: String,
    table: TableName,
    key_columns: IndexElementCollection,
    include_columns: IndexElementCollection,
    unique: bool,
}

impl CreateIndex {
    /// create a instance
    #[must_use]
    pub fn new(name: &str, table: &TableName, key_columns: &IndexElementCollection) -> Self {
        Self {
            name: name.to_string(),
            table: table.clone(),
            key_columns: key_columns.clone(),
            include_columns: IndexElementCollection::default(),
            unique: true,
        }
    }

    /// create with `include_columns`
    #[must_use]
    pub fn new_with_include_column(
        name: &str,
        table: &TableName,
        key_columns: &IndexElementCollection,
        include_columns: &IndexElementCollection,
    ) -> Self {
        Self {
            name: name.to_string(),
            table: table.clone(),
            key_columns: key_columns.clone(),
            include_columns: include_columns.clone(),
            unique: true,
        }
    }

    /// get index name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    fn sort_order(key: &crate::syn::IndexElem) -> Result<SortOrder, AnalysisError> {
        let order = if let Some(order) = key.get_ordering().as_inner() {
            match order {
                SortByDir::SortbyUsing => {
                    AnalysisError::raise_unexpected_input("SortbyUsing in IndexElem")?
                }
                SortByDir::SortbyDesc => SortOrder::Desc,
                _ => SortOrder::Asc,
            }
        } else {
            SortOrder::Asc
        };
        Ok(order)
    }

    fn analyze_index_elem<TParseContext>(
        mut context: TParseContext,
        keys: Vec<crate::syn::IndexElem>,
    ) -> Result<(IndexElementCollection, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let mut key_columns = IndexElementCollection::default();
        for key in keys {
            let element = if let Some(expr) = key.get_expr().as_inner() {
                let (scaler_expr, new_context) = analyze_scaler_expr(
                    context,
                    &WithClause::default(),
                    &FromClause::default(),
                    expr,
                )?;
                context = new_context;
                IndexElement::from(&scaler_expr)
            } else {
                let name = key.get_name();
                IndexElement::from(&ColumnName::from(name))
            };

            let order = CreateIndex::sort_order(&key)?;
            let col_name = key.get_indexcolname();
            let col_name = if col_name.is_empty() {
                None
            } else {
                Some(col_name)
            };

            key_columns.push(IndexElementWithOrder::new(&element, order, &col_name));
        }
        Ok((key_columns, context))
    }
}

/// analyze [`crate::syn::IndexStmt`]
pub fn analyze_index_stmt<TParseContext>(
    mut context: TParseContext,
    _parent_schema: &Option<String>,
    syn: crate::syn::IndexStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(table) = syn.get_relation().as_inner() else {
        AnalysisError::raise_unexpected_none("indexstmt.relation")?
    };
    let Some(keys) = syn.get_index_params().map(|v| v.as_index_elem()) else {
        AnalysisError::raise_unexpected_none("indexstmt.index_params")?
    };

    let name = syn.get_idxname();
    let table = TableName::try_from(table)?;
    let unique = syn.get_unique();

    let (key_columns, new_context) = CreateIndex::analyze_index_elem(context, keys)?;
    context = new_context;

    let include_columns =
        if let Some(includes) = syn.get_index_including_params().map(|v| v.as_index_elem()) {
            let (cols, new_context) = CreateIndex::analyze_index_elem(context, includes)?;
            context = new_context;
            cols
        } else {
            IndexElementCollection::default()
        };

    let create_index = CreateIndex {
        name,
        table,
        key_columns,
        include_columns,
        unique,
    };

    context = context.apply_create_index(&create_index)?;

    Ok(AstAndContextPair::new(
        SemAst::CreateIndex(create_index),
        context,
    ))
}
