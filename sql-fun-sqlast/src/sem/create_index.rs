mod arbiter;
mod settings;

pub use self::settings::IndexStorageValue;

#[cfg(test)]
use crate::syn::IndexElem;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::SortOrder as SqliteSortOrder;
use crate::{
    ast::Index as AstIndex,
    sem::{
        ColumnRename,
        problem::IndexProblemKind,
        traversal::{ColumnRenamer, RelationRenamer, SemanticVisitorMut, optional},
    },
    syn::IndexStmt,
};

use sql_fun_core::IVec;

use crate::{
    ast::TryFromWithTokens,
    names::{ColumnName, FullName, TableName},
    sem::{AnalysisError, AstAndContextPair, ParseContext, SemScalarExpr},
    syn::ScanToken,
};

/// Index item sort order
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub enum SortOrder {
    /// ascending
    Asc,
    /// dessending
    Desc,
}

#[cfg(feature = "sqlite")]
impl From<SqliteSortOrder> for SortOrder {
    fn from(value: SqliteSortOrder) -> Self {
        #[cfg(feature = "sqlite")]
        use crate::syn::sqlite::SortOrder;

        match value {
            SortOrder::Asc => Self::Asc,
            SortOrder::Desc => Self::Desc,
        }
    }
}

/// `IndexElement` has column or expression
#[derive(Debug, Clone)]
pub enum IndexElement {
    /// column valiant
    ColumnName(ColumnName),
    /// expression variant
    Expr(Box<SemScalarExpr>),
}

impl IndexElement {
    fn rename_column(&mut self, rename: &ColumnRename) -> Result<(), AnalysisError> {
        match self {
            Self::ColumnName(name) => rename.rename_name(name),
            Self::Expr(value) => value.walk(&mut ColumnRenamer::new(rename))?,
        }
        Ok(())
    }

    fn rename_relation(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        if let Self::Expr(expression) = self {
            expression.walk(visitor)?;
        }
        Ok(())
    }

    const fn is_expression(&self) -> bool {
        matches!(self, Self::Expr(_))
    }
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
    fn rename_relation(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        for column in &mut self.0 {
            column.element.rename_relation(visitor)?;
        }
        Ok(())
    }

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

    pub(crate) fn push(&mut self, item: IndexElementWithOrder) {
        self.0.push(item);
    }
}

#[cfg(test)]
mod index_element_collection_tests {
    use crate::{
        names::ColumnName,
        sem::create_index::{
            IndexElement, IndexElementCollection, IndexElementWithOrder, SortOrder,
        },
    };
    use ::sql_fun_core::IVec;

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
    if_not_exists: bool,
    access_method: String,
    storage_parameters: std::collections::BTreeMap<String, IndexStorageValue>,
    statistics: std::collections::BTreeMap<i32, i32>,

    name: FullName,
    table: TableName,
    key_columns: IndexElementCollection,
    include_columns: IndexElementCollection,
    unique: bool,
    predicate: Option<SemScalarExpr>,
}

impl CreateIndex {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename) -> Result<(), AnalysisError> {
        if rename.matches_table(&self.table) {
            for column in self
                .key_columns
                .0
                .iter_mut()
                .chain(self.include_columns.0.iter_mut())
            {
                column.element.rename_column(rename)?;
            }
            optional(&mut self.predicate, &mut ColumnRenamer::new(rename))?;
        }
        Ok(())
    }

    pub(crate) fn rename_relation(
        &mut self,
        old: &TableName,
        new: &TableName,
    ) -> Result<(), AnalysisError> {
        self.table.rename_reference(old, new);
        let mut visitor = RelationRenamer::new(old, new);
        self.key_columns.rename_relation(&mut visitor)?;
        self.include_columns.rename_relation(&mut visitor)?;
        optional(&mut self.predicate, &mut visitor)
    }

    /// Index access method retained from CREATE INDEX.
    pub fn access_method(&self) -> &str {
        &self.access_method
    }
    pub(crate) fn set_access_method(&mut self, method: &str) {
        self.access_method = if method.is_empty() {
            "btree".to_owned()
        } else {
            method.to_owned()
        };
    }
    /// Explicit storage override; None means the database default is in effect.
    pub fn storage_parameter(&self, name: &str) -> Option<&IndexStorageValue> {
        self.storage_parameters.get(name)
    }
    /// Explicit expression-column statistics target; None means the system default.
    pub fn statistics_target(&self, column: i32) -> Option<i32> {
        self.statistics.get(&column).copied()
    }
    pub(crate) fn apply_statistics(&mut self, column: i32, target: i32) {
        if target == -1 {
            self.statistics.remove(&column);
        } else {
            self.statistics.insert(column, target);
        }
    }
    pub(crate) fn statistics_column_problem(&self, column: i32) -> Option<IndexProblemKind> {
        use crate::sem::problem::IndexProblemKind;
        let position = usize::try_from(column - 1).ok()?;
        if let Some(element) = self.key_columns.0.get(position) {
            if element.element.is_expression() {
                None
            } else {
                Some(IndexProblemKind::non_expression_column())
            }
        } else if position < self.key_columns.0.len() + self.include_columns.0.len() {
            Some(IndexProblemKind::included_column())
        } else {
            Some(IndexProblemKind::invalid_column())
        }
    }

    pub(crate) fn contains_name(indexes: &[Self], name: &FullName) -> bool {
        indexes.iter().any(|index| index.name() == name)
    }
    pub(crate) fn with_table_identity(mut self, table: &FullName) -> Self {
        self.table = TableName::from(table.clone());
        self.name = FullName::new(
            &table.catalog().cloned(),
            &table.schema().cloned(),
            self.name.name(),
        );
        self
    }

    pub(crate) const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    pub(crate) const fn with_if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }

    pub(crate) fn set_predicate(&mut self, predicate: Option<SemScalarExpr>) {
        self.predicate = predicate;
    }

    pub(crate) fn belongs_to(&self, table: &TableName) -> bool {
        &self.table == table
    }
    pub(crate) fn from_resolved(
        name: &FullName,
        table: &TableName,
        keys: &IndexElementCollection,
        includes: &IndexElementCollection,
        unique: bool,
    ) -> Self {
        Self {
            if_not_exists: false,
            access_method: "btree".to_owned(),
            storage_parameters: std::collections::BTreeMap::new(),
            statistics: std::collections::BTreeMap::new(),
            name: name.clone(),
            table: table.clone(),
            key_columns: keys.clone(),
            include_columns: includes.clone(),
            unique,
            predicate: None,
        }
    }
    /// Create an index with its qualified catalog identity and resolved target table.
    #[must_use]
    pub fn new(name: &FullName, table: &TableName, key_columns: &IndexElementCollection) -> Self {
        Self {
            if_not_exists: false,
            access_method: "btree".to_owned(),
            storage_parameters: std::collections::BTreeMap::new(),
            statistics: std::collections::BTreeMap::new(),
            name: name.clone(),
            table: table.clone(),
            key_columns: key_columns.clone(),
            include_columns: IndexElementCollection::default(),
            unique: true,
            predicate: None,
        }
    }

    /// Create a qualified index with `include_columns`
    #[must_use]
    pub fn new_with_include_column(
        name: &FullName,
        table: &TableName,
        key_columns: &IndexElementCollection,
        include_columns: &IndexElementCollection,
    ) -> Self {
        Self {
            if_not_exists: false,
            access_method: "btree".to_owned(),
            storage_parameters: std::collections::BTreeMap::new(),
            statistics: std::collections::BTreeMap::new(),
            name: name.clone(),
            table: table.clone(),
            key_columns: key_columns.clone(),
            include_columns: include_columns.clone(),
            unique: true,
            predicate: None,
        }
    }

    /// Return the qualified catalog identity, including the target table schema.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }
}

#[cfg(test)]
mod create_index_tests {
    use crate::{
        names::{ColumnName, FullName, TableName},
        sem::create_index::{CreateIndex, IndexElementCollection},
    };

    #[test]
    fn new_sets_defaults_and_clones_fields() {
        let table = TableName::from(FullName::with_schema("public", "items"));
        let key_columns =
            IndexElementCollection::from_column_names(&vec![ColumnName::from("id")].into());

        let index = CreateIndex::new(
            &FullName::with_schema("public", "idx_items_id"),
            &table,
            &key_columns,
        );

        assert_eq!(index.name, FullName::with_schema("public", "idx_items_id"));
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
            &FullName::with_schema("public", "idx_items_id"),
            &table,
            &key_columns,
            &include_columns,
        );

        assert_eq!(index.key_columns.0.len(), 1);
        assert_eq!(index.include_columns.0.len(), 1);
        assert!(index.unique);
    }
}

#[cfg(test)]
impl CreateIndex {
    fn analyze_index_elem<TContext: ParseContext>(
        mut context: TContext,
        keys: Vec<IndexElem>,
        tokens: &IVec<ScanToken>,
    ) -> Result<(IndexElementCollection, TContext), AnalysisError> {
        use crate::ast::{Index, IndexExpression};
        let keys = keys
            .iter()
            .map(|key| IndexExpression::try_from_with_tokens(key, tokens))
            .collect::<Result<Vec<_>, _>>()?;
        context.begin_parameter_type_analysis();
        let result = Index::resolve_elements(&keys, &mut context);
        let elements = context.finish_analysis(result)?;
        Ok((elements, context))
    }
}

#[cfg(test)]
mod analyze_index_elem_tests {
    use crate::{
        names::ColumnName,
        sem::{
            ScalarConstExpr, SemScalarExpr,
            create_index::{CreateIndex, IndexElement, SortOrder},
        },
        syn::{Node, NodeInner, ScanToken, SortByDir},
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
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: IndexStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        AstIndex::try_from_with_tokens(&syn, tokens)?.resolve_and_apply(context)
    }
}

#[cfg(test)]
mod analyze_index_stmt_tests {
    use crate::test_helpers::source_span;

    use crate::{
        names::{ColumnName, FullName},
        sem::{
            AstAndContextPair, SemAst,
            create_index::{CreateIndex, IndexElement, SortOrder},
        },
        syn::{AliasOpt, RangeVarOpt, ScanToken, SortByDir},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use sql_fun_core::IVec;

    #[rstest::rstest]
    fn analyze_index_stmt_collects_keys_including_order_and_includes(
        mut test_context: TestParseContext,
    ) {
        test_context.setup_table("items", &[], &source_span());
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

        assert_eq!(
            create_index.name,
            FullName::with_schema("public", "idx_items")
        );
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
