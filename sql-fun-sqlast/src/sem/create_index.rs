use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AstAndContextPair, ColumnName, FromClause, ParseContext, SemAst,
        SemScalarExpr, TableName, WithClause,
    },
    syn::{ListOpt, Opt, ScanToken, SortByDir},
};

/// Index item sort order
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
pub struct IndexElementWithOrder {
    #[cfg_attr(not(test), expect(dead_code))]
    element: IndexElement,
    #[cfg_attr(not(test), expect(dead_code))]
    col_name: Option<String>,
    #[cfg_attr(not(test), expect(dead_code))]
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

#[cfg(test)]
mod index_element_collection_tests {
    use super::*;

    #[test]
    fn from_column_names_uses_asc_order_and_no_alias() {
        let columns: IVec<ColumnName> =
            vec![ColumnName::from("id"), ColumnName::from("name")].into();

        let collection = IndexElementCollection::from_column_names(&columns);

        assert_eq!(collection.0.len(), 2);
        match &collection.0[0] {
            IndexElementWithOrder {
                element: IndexElement::ColumnName(name),
                col_name,
                order,
            } => {
                assert_eq!(name, &ColumnName::from("id"));
                assert!(col_name.is_none());
                assert!(matches!(order, SortOrder::Asc));
            }
            _ => panic!("expected column name index element"),
        }
        match &collection.0[1] {
            IndexElementWithOrder {
                element: IndexElement::ColumnName(name),
                col_name,
                order,
            } => {
                assert_eq!(name, &ColumnName::from("name"));
                assert!(col_name.is_none());
                assert!(matches!(order, SortOrder::Asc));
            }
            _ => panic!("expected column name index element"),
        }
    }
}

/// analyzed [`crate::syn::IndexStmt`]
#[derive(Debug, Clone)]
pub struct CreateIndex {
    name: String,
    #[cfg_attr(not(test), expect(dead_code))]
    table: TableName,
    #[cfg_attr(not(test), expect(dead_code))]
    key_columns: IndexElementCollection,
    #[cfg_attr(not(test), expect(dead_code))]
    include_columns: IndexElementCollection,
    #[cfg_attr(not(test), expect(dead_code))]
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
}

#[cfg(test)]
mod create_index_tests {
    use super::*;
    use crate::sem::FullName;

    #[test]
    fn new_sets_defaults_and_clones_fields() {
        let table = TableName::from(FullName::with_schema("public", "items"));
        let key_columns =
            IndexElementCollection::from_column_names(&vec![ColumnName::from("id")].into());

        let index = CreateIndex::new("idx_items_id", &table, &key_columns);

        assert_eq!(index.name, "idx_items_id");
        assert_eq!(index.table, table);
        assert_eq!(index.key_columns.0.len(), 1);
        assert_eq!(index.include_columns.0.len(), 0);
        assert!(index.unique);
    }

    #[test]
    fn new_with_include_column_preserves_include_columns() {
        let table = TableName::from(FullName::with_schema("public", "items"));
        let key_columns =
            IndexElementCollection::from_column_names(&vec![ColumnName::from("id")].into());
        let include_columns =
            IndexElementCollection::from_column_names(&vec![ColumnName::from("metadata")].into());

        let index = CreateIndex::new_with_include_column(
            "idx_items_id",
            &table,
            &key_columns,
            &include_columns,
        );

        assert_eq!(index.key_columns.0.len(), 1);
        assert_eq!(index.include_columns.0.len(), 1);
        assert!(index.unique);
    }
}

impl CreateIndex {
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
        tokens: &IVec<ScanToken>,
    ) -> Result<(IndexElementCollection, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let mut key_columns = IndexElementCollection::default();
        for key in keys {
            let element = if let Some(expr) = key.get_expr().as_inner() {
                let (scaler_expr, new_context) = SemScalarExpr::analyze_scaler_expr(
                    context,
                    &WithClause::default(),
                    &FromClause::default(),
                    expr,
                    tokens,
                )?;
                context = new_context;
                IndexElement::from(&scaler_expr)
            } else {
                let name = key.get_name();
                IndexElement::from(&ColumnName::from(name))
            };

            let order = Self::sort_order(&key)?;
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

#[cfg(test)]
mod analyze_index_elem_tests {
    use super::*;
    use crate::{
        sem::ScalarConstExpr,
        syn::{Node, NodeInner, SortByDir},
        test_helpers::{SynBuilder, TestParseContext},
    };
    use sql_fun_core::IVec;

    #[test]
    fn analyze_index_elem_collects_column_order_and_alias() {
        let builder = SynBuilder::new();
        let keys = vec![
            builder.index_elem_column("id", SortByDir::SortbyDesc.into(), Some("id_alias")),
            builder.index_elem_column("name", SortByDir::SortbyDefault.into(), None),
        ];
        let tokens: IVec<ScanToken> = IVec::default();
        let context = TestParseContext::default();

        let (collection, _context) =
            CreateIndex::analyze_index_elem(context, keys, &tokens).expect("analyze index elem");

        assert_eq!(collection.0.len(), 2);

        let first = &collection.0[0];
        assert!(matches!(first.order, SortOrder::Desc));
        assert_eq!(first.col_name.as_deref(), Some("id_alias"));
        match &first.element {
            IndexElement::ColumnName(name) => assert_eq!(name, &ColumnName::from("id")),
            _ => panic!("expected column name"),
        }

        let second = &collection.0[1];
        assert!(matches!(second.order, SortOrder::Asc));
        assert!(second.col_name.is_none());
        match &second.element {
            IndexElement::ColumnName(name) => assert_eq!(name, &ColumnName::from("name")),
            _ => panic!("expected column name"),
        }
    }

    #[test]
    fn analyze_index_elem_collects_expression() {
        let builder = SynBuilder::new();
        let expr_node = Node::from(NodeInner::AConst(builder.const_int4(42)));
        let keys = vec![builder.index_elem_expr(expr_node, SortByDir::SortbyDefault.into(), None)];
        let tokens: IVec<ScanToken> = IVec::default();
        let context = TestParseContext::default();

        let (collection, _context) =
            CreateIndex::analyze_index_elem(context, keys, &tokens).expect("analyze index elem");

        assert_eq!(collection.0.len(), 1);
        let item = &collection.0[0];
        assert!(matches!(item.order, SortOrder::Asc));
        match &item.element {
            IndexElement::Expr(expr) => match expr.as_ref() {
                SemScalarExpr::Const(ScalarConstExpr::Integer(value)) => {
                    assert_eq!(*value, 42);
                }
                _ => panic!("expected integer const expr"),
            },
            _ => panic!("expected expression element"),
        }
    }
}

impl CreateIndex {
    /// analyze [`crate::syn::IndexStmt`]
    pub fn analyze_index_stmt<TParseContext>(
        mut context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::IndexStmt,
        tokens: &IVec<ScanToken>,
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

        let (key_columns, new_context) = Self::analyze_index_elem(context, keys, tokens)?;
        context = new_context;

        let include_columns =
            if let Some(includes) = syn.get_index_including_params().map(|v| v.as_index_elem()) {
                let (cols, new_context) = Self::analyze_index_elem(context, includes, tokens)?;
                context = new_context;
                cols
            } else {
                IndexElementCollection::default()
            };

        let create_index = Self {
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
}

/// analyze [`crate::syn::IndexStmt`]
#[deprecated = "use CreateIndex::analyze_index_stmt"]
pub fn analyze_index_stmt<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::IndexStmt,
    tokens: &IVec<ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    CreateIndex::analyze_index_stmt(context, parent_schema, syn, tokens)
}

#[cfg(test)]
mod analyze_index_stmt_tests {
    use super::*;
    use crate::{
        sem::FullName,
        syn::{AliasOpt, RangeVarOpt, SortByDir},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use rstest::rstest;
    use sql_fun_core::IVec;

    #[rstest]
    fn analyze_index_stmt_collects_keys_including_order_and_includes(
        test_context: TestParseContext,
    ) {
        let builder = SynBuilder::new();
        let relation = RangeVarOpt::from(
            builder.range_var(&FullName::with_schema("public", "items"), &AliasOpt::none()),
        );
        let index_params = builder.index_elem_list(vec![
            builder.index_elem_column("id", SortByDir::SortbyDesc.into(), None),
            builder.index_elem_column("name", SortByDir::SortbyDefault.into(), None),
        ]);
        let include_params = builder.index_elem_list(vec![builder.index_elem_column(
            "metadata",
            SortByDir::SortbyDefault.into(),
            None,
        )]);
        let index_stmt =
            builder.index_stmt("idx_items", relation, index_params, include_params, false);
        let tokens: IVec<ScanToken> = IVec::default();

        let AstAndContextPair(ast, context) =
            CreateIndex::analyze_index_stmt(test_context, &None, index_stmt, &tokens)
                .expect("analyze index");

        let SemAst::CreateIndex(create_index) = ast else {
            panic!("expected create index ast");
        };

        assert_eq!(create_index.name, "idx_items");
        assert!(matches!(
            create_index.table.full_name().schema(),
            Some(schema) if schema.as_str() == "public"
        ));
        assert!(!create_index.unique);
        assert_eq!(create_index.key_columns.0.len(), 2);
        assert_eq!(create_index.include_columns.0.len(), 1);

        let first_key = &create_index.key_columns.0[0];
        assert!(matches!(first_key.order, SortOrder::Desc));
        match &first_key.element {
            IndexElement::ColumnName(name) => assert_eq!(name, &ColumnName::from("id")),
            _ => panic!("expected column name"),
        }

        let second_key = &create_index.key_columns.0[1];
        assert!(matches!(second_key.order, SortOrder::Asc));
        match &second_key.element {
            IndexElement::ColumnName(name) => assert_eq!(name, &ColumnName::from("name")),
            _ => panic!("expected column name"),
        }

        let include_key = &create_index.include_columns.0[0];
        match &include_key.element {
            IndexElement::ColumnName(name) => assert_eq!(name, &ColumnName::from("metadata")),
            _ => panic!("expected column name"),
        }

        assert_eq!(context.create_indexes_len(), 1);
        let applied = context.last_create_index().expect("applied create index");
        assert_eq!(applied.name, create_index.name);
    }
}
