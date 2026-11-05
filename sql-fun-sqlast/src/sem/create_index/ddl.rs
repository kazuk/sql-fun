//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::{IndexExpression as AstIndexExpression, ddl::Index as AstIndex},
    context::ParseContext,
    names::{ColumnName, FullName, LocalName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CreateIndex, DataSource, FromClause,
        IndexElement, IndexElementCollection, IndexElementWithOrder, IndexStorageChange,
        IndexStorageParameter, ScalarAnalysisContext, SemAst, SemScalarExpr, SortOrder, WithClause,
    },
};

impl CreateIndex {
    #[cfg(test)]
    pub(crate) fn elements_from_ast(
        elements: &[AstIndexExpression],
        context: &mut impl ParseContext,
    ) -> Result<IndexElementCollection, AnalysisError> {
        Self::elements_from_ast_in(elements, context, &ScalarAnalysisContext::default())
    }

    pub(crate) fn elements_from_ast_in(
        elements: &[AstIndexExpression],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<IndexElementCollection, AnalysisError> {
        let mut collection = IndexElementCollection::default();
        for element in elements {
            if element.using_order() {
                return AnalysisError::raise_unexpected_input("SortbyUsing in IndexElem");
            }
            let value = if let Some(expr) = &element.expression() {
                IndexElement::from(&SemScalarExpr::from_ast_in(expr, context, scope)?)
            } else {
                IndexElement::from(&ColumnName::from(
                    element.name().as_deref().unwrap_or_default(),
                ))
            };
            let order = if element.descending() == Some(true) {
                SortOrder::Desc
            } else {
                SortOrder::Asc
            };
            collection.push(IndexElementWithOrder::new(&value, order, element.alias()));
        }
        Ok(collection)
    }
}

#[cfg(test)]
mod test_analyze_index_elem_collects_column_order_and_alias {
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
    pub(crate) fn from_ast(
        ast: &AstIndex,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context);
        context.finish_analysis(result)
    }

    pub(crate) fn from_ast_inner(
        ast: &AstIndex,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let requested_table = TableName::from(FullName::from_ast_name(ast.relation().name())?);
        let Some(table) = context.resolve_table_name(&requested_table) else {
            context
                .report_analysis_problem(AnalysisProblem::relation_not_found(&requested_table))?;
            return Ok(None);
        };
        let name = FullName::new(
            &table.full_name().catalog().cloned(),
            &table.full_name().schema().cloned(),
            &LocalName::from(ast.name().as_str()),
        );
        let alias = AliasSpec::from_local_name(&table.full_name().local_name());
        let with = WithClause::default();
        let mut from = FromClause::default();
        from.push(DataSource::resolve_relation(
            context,
            table.full_name(),
            &alias,
            &with,
        )?);
        let scope = ScalarAnalysisContext::nested(&with, &from, None);
        let keys = Self::elements_from_ast_in(ast.keys(), context, &scope)?;
        let includes = Self::elements_from_ast_in(ast.includes(), context, &scope)?;
        let predicate = SemScalarExpr::from_ast_optional(ast.predicate(), context, &scope)?;
        let mut index = Self::from_resolved(&name, &table, &keys, &includes, ast.unique());
        index = index.with_if_not_exists(ast.if_not_exists());
        index.set_predicate(predicate);
        if !Self::retain_ast_storage(ast, context, &mut index)? {
            return Ok(None);
        }
        Ok(Some(index))
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstIndex,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(index) = Self::from_ast(ast, &mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = context.apply_create_index(&index)?;
        Ok(AstAndContextPair::new(
            SemAst::create_index(&index),
            context,
        ))
    }
}

#[cfg(test)]
mod test_analyze_index_stmt_collects_keys_including_order_and_includes {
    use crate::{
        names::{ColumnName, FullName},
        sem::{
            AstAndContextPair, SemAst,
            create_index::{CreateIndex, IndexElement, SortOrder},
        },
        syn::{AliasOpt, RangeVarOpt, ScanToken, SortByDir},
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
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

impl CreateIndex {
    pub(crate) fn retain_ast_storage(
        ast: &AstIndex,
        context: &mut impl ParseContext,
        index: &mut Self,
    ) -> Result<bool, AnalysisError> {
        index.set_access_method(ast.access_method());
        if index.access_method() != "btree" {
            return Ok(true);
        }
        let parameters = IndexStorageParameter::from_ast_list(ast.options())?;
        let problems = IndexStorageChange::new(false, &parameters).evaluate(index);
        let mut valid = true;
        for problem in problems {
            if !problem.is_warning() {
                valid = false;
            }
            context.report_analysis_problem(AnalysisProblem::index(&problem))?;
        }
        Ok(valid)
    }
}
