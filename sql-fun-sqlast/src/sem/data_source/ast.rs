use crate::{
    ast::{
        Call, DerivedTable, Dialect, Expr, FromGroup, FromItem, Join, Relation, TableFunctions,
        TableSample, XmlTable,
    },
    context::ParseContext,
    names::FullName,
    sem::{
        AliasSpec, AnalysisError, CreateTrigger as SemCreateTrigger, DataSource,
        DataSourceRowsFromFunc, FromClause, FuncCallExpr, FunctionModifiers,
        JoinKind as SemanticJoinKind, JoinUsingClause, ResolutionFailure, ScalarAnalysisContext,
        SelectStatement, SemScalarExpr, TableFunctionOutput, TableSampleSource, WithClause,
        with_stack,
    },
};

impl DataSource {
    pub(crate) fn from_ast_xml_table(
        ast: &XmlTable,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_table_call(
            ast.rowexpr(),
            context,
            scope,
            ast.alias().as_ref().map(AliasSpec::from_ast).as_ref(),
        )
    }
}

impl DataSource {
    pub(crate) fn from_ast(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        preceding: &FromClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let scope = ScalarAnalysisContext::nested(with, preceding, outer);
        Self::from_ast_in_scope(ast, context, with, &scope)
    }
}

impl DataSource {
    pub(crate) fn from_ast_in_scope(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        with_stack(|| Self::from_ast_body(ast, context, with, scope))
    }
}

impl DataSource {
    #[inline(never)]
    pub(crate) fn from_ast_body(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            FromItem::Relation(relation) => Self::from_ast_relation(relation, context, with, scope),
            FromItem::Query(derived) => {
                Self::from_ast_derived_table(derived.as_ref(), context, scope)
            }
            FromItem::Join(join) => Self::from_ast_join(join.as_ref(), context, with, scope),
            FromItem::Functions(functions) => {
                Self::from_ast_table_functions(functions, context, scope)
            }
            FromItem::Group(group) => {
                Self::from_ast_from_group(group.as_ref(), context, with, scope)
            }
            FromItem::Xml(table) => Self::from_ast_xml_table(table.as_ref(), context, scope),
            FromItem::Sample(sample) => {
                Self::from_ast_table_sample(sample.as_ref(), context, with, scope)
            }
            FromItem::Json(_) => AnalysisError::raise_unexpected_none("from_clause inner node"),
        }
    }
}

impl DataSource {
    pub(crate) fn from_ast_table_functions(
        ast: &TableFunctions,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        if ast.rows_from() {
            return Self::from_ast_rows_from(ast, context, scope);
        }
        let [function] = ast.calls().as_slice() else {
            return AnalysisError::raise_unexpected_input("table function count");
        };
        let mut output = TableFunctionOutput::from_ast_typed_columns(ast.columns(), context)?;
        if ast.ordinality() && !ast.columns().is_empty() {
            output.reject_placement(context)?;
        }
        let mut source = Self::from_ast_table_call(
            function.expression(),
            context,
            scope,
            ast.alias().as_ref().map(AliasSpec::from_ast).as_ref(),
        )?;
        source.apply_function_output(context, &output)?;
        if ast.ordinality() {
            source.append_function_ordinality();
        }
        source.validate_function_aliases(context)?;
        Ok(source)
    }
}

#[cfg(test)]
mod test_analyze_range_func {
    use crate::{
        names::SchemaName,
        sem::{DataSource, FromClause, WithClause},
        test_helpers::{TestParseContext, setup_function_overload, source_syntax, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_func(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        test_context.setup_function("set_func", std::slice::from_ref(&overload));

        let syntax = source_syntax::function("set_func", "set_alias", false)?;
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let ds =
            DataSource::from_ast_source(&syntax, &mut test_context, &with_clause, &from_clause)?;

        let mut aliases = Vec::new();
        ds.push_alias_names(&mut aliases);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name().as_str(), "set_alias");
        let DataSource::CallTableFunc(table_func) = ds else {
            panic!("expected table function datasource");
        };
        assert_eq!(table_func.function(), &overload);
        Ok(())
    }
}

impl DataSource {
    pub(crate) fn from_ast_rows_from(
        ast: &TableFunctions,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut outer_output = TableFunctionOutput::from_ast_typed_columns(ast.columns(), context)?;
        let outer_allowed =
            ast.calls().len() == 1 && !ast.ordinality() && ast.calls()[0].columns().is_empty();
        if !ast.columns().is_empty() && !outer_allowed {
            outer_output.reject_placement(context)?;
        }
        let invocation_alias = if ast.calls().len() == 1 {
            ast.alias()
                .as_ref()
                .map(|value| AliasSpec::from_names(value.name(), &[]))
        } else {
            None
        };
        let mut functions = Vec::new();
        for function in ast.calls() {
            let local_output =
                TableFunctionOutput::from_ast_typed_columns(function.columns(), context)?;
            let output = if outer_allowed && !ast.columns().is_empty() {
                &outer_output
            } else {
                &local_output
            };
            let mut function = DataSourceRowsFromFunc::from_ast_call(
                function.expression(),
                context,
                scope,
                invocation_alias.as_ref(),
            )?;
            function.apply_output(context, output)?;
            functions.push(function);
        }
        let alias = ast
            .alias()
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| {
                AliasSpec::from_local_name(
                    &functions
                        .first()
                        .map(|function| function.default_alias())
                        .unwrap_or_default(),
                )
            });
        let mut source = Self::resolved_rows(&functions, &alias);
        source.retain_rows_definition(&outer_output);
        if ast.ordinality() {
            source.append_function_ordinality();
        }
        source.validate_function_aliases(context)?;
        Ok(source)
    }
}

#[cfg(test)]
mod test_analyze_range_func_rows_from {
    use crate::{
        names::{FullName, SchemaName},
        sem::{
            AliasSpec, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc, FromClause,
            WithClause,
        },
        test_helpers::{TestParseContext, setup_function_overload, source_syntax, test_context},
    };

    use ::sql_fun_core::IVec;

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_func_rows_from(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        let func_name = test_context.setup_function("rows_func", std::slice::from_ref(&overload));

        let syntax = source_syntax::function(func_name.local_name().as_str(), "rows_alias", true)?;
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let ds =
            DataSource::from_ast_source(&syntax, &mut test_context, &with_clause, &from_clause)?;

        let mut invocation = DataSource::table_function(
            &overload,
            IVec::default(),
            &AliasSpec::from_names("rows_alias", &[]),
        );
        invocation.record_function_name(
            &FullName::from_local_name(&func_name.local_name()),
            &func_name,
        );
        let expected_rows_func = DataSourceRowsFromFunc::invocation(&invocation);
        let expected_functions: IVec<_> = vec![expected_rows_func].into();
        let expected_alias = AliasSpec::from_names("rows_alias", &[]);
        let expected_row_functions =
            DataSourceRowFunctions::new(&expected_functions, &expected_alias);
        let expected = DataSource::RowFunctions(expected_row_functions);
        assert_eq!(expected, ds);
        Ok(())
    }
}

impl DataSource {
    pub(crate) fn from_ast_expression_table(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        let function = DataSourceRowsFromFunc::from_ast_call(ast, context, scope, alias)?;
        let alias = alias
            .cloned()
            .unwrap_or_else(|| AliasSpec::from_local_name(&function.default_alias()));
        Ok(Self::resolved_rows(&[function], &alias))
    }
}

impl DataSource {
    pub(crate) fn from_ast_table_call(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Expr::Call(call) if SemScalarExpr::ast_is_ordinary(&call.kind()) => {
                Self::from_ast_table(call.as_ref(), context, scope, alias)
            }
            _ => Self::from_ast_expression_table(ast, context, scope, alias),
        }
    }
}

#[cfg(test)]
mod test_analyze_range_table_func {
    use crate::{
        names::SchemaName,
        sem::{DataSource, FromClause, WithClause},
        test_helpers::{TestParseContext, setup_function_overload, source_syntax, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_table_func(mut test_context: TestParseContext) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let (int4, overload) = setup_function_overload();
        test_context.setup_type(int4);
        test_context.setup_function("table_func", std::slice::from_ref(&overload));

        let syntax = source_syntax::table_function("table_func", "func_alias")?;
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let ds =
            DataSource::from_ast_source(&syntax, &mut test_context, &with_clause, &from_clause)?;

        let mut aliases = Vec::new();
        ds.push_alias_names(&mut aliases);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name().as_str(), "func_alias");
        let DataSource::CallTableFunc(table_func_ds) = ds else {
            panic!("expected table function datasource");
        };
        assert_eq!(table_func_ds.function(), &overload);
        Ok(())
    }
}

impl DataSource {
    pub(crate) fn from_ast_table(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&AliasSpec>,
    ) -> Result<Self, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            todo!("SQLite table function affinity and overload semantics");
        }
        let mut arguments = SemScalarExpr::from_ast_list(ast.arguments(), context, scope)?;
        let mut invalid_arguments = false;
        for argument in &mut arguments {
            invalid_arguments |= DataSourceRowsFromFunc::validate_argument(context, argument)?;
        }
        let name = FullName::from_ast_name(ast.name())?;
        let alias = alias
            .cloned()
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        if SemScalarExpr::ast_uses_supplied_binding(ast) {
            let mut call = FuncCallExpr::from_ast_supplied_call(ast, context, &arguments)?;
            call.set_modifiers(FunctionModifiers::from_ast_modifiers(ast, context, scope)?);
            let mut source = Self::deferred_table_function(context, &call, &alias);
            source.validate_table_function(context, invalid_arguments)?;
            return Ok(source);
        }
        let mut source = Self::resolve_function(context, &name, &arguments, &alias)?;
        source.validate_table_function(context, invalid_arguments)?;
        Ok(source)
    }
}

impl DataSource {
    pub(crate) fn from_ast_list_in(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        Self::from_ast_list_in_scope(items, context, with, &scope)
    }
}

impl DataSource {
    pub(crate) fn from_ast_list_in_scope(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<FromClause, AnalysisError> {
        let mut from = FromClause::default();
        for item in items {
            let source =
                Self::from_ast_in_scope(item, context, with, &scope.with_from_clause(&from))?;
            from.validate_source(context, &source)?;
            from.push(source);
        }

        Ok(from)
    }
}

impl DataSource {
    #[cfg(test)]
    pub(crate) fn from_ast_list(
        items: &[FromItem],
        context: &mut impl ParseContext,
        with: &WithClause,
    ) -> Result<FromClause, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_list_in(items, context, with, None);
        context.finish_analysis(result)
    }
}

impl DataSource {
    #[cfg(test)]
    pub(crate) fn from_ast_source(
        ast: &FromItem,
        context: &mut impl ParseContext,
        with: &WithClause,
        preceding: &FromClause,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, context, with, preceding, None);
        context.finish_analysis(result)
    }
}

impl DataSource {
    pub(crate) fn from_ast_relation(
        ast: &Relation,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(ast.name())?;
        let name = if with.resolve_name(context, &name).is_none() {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let alias = ast
            .alias()
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        let source = Self::resolve_relation(context, &name, &alias, with)?;
        Ok(source)
    }
}

impl DataSource {
    pub(crate) fn from_ast_derived_table(
        ast: &DerivedTable,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let non_lateral_scope;
        let scope = if ast.lateral() {
            scope
        } else {
            non_lateral_scope = scope.with_from_clause(&FromClause::default());
            &non_lateral_scope
        };
        let query = SelectStatement::from_ast_in(ast.query(), context, Some(scope))?;
        let alias = ast
            .alias()
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(""));
        Ok(Self::resolved_query(&query, &alias))
    }
}

#[cfg(test)]
mod test_analyze_range_subselect {
    use crate::{
        context::MutSourcePathWithSpan,
        sem::{DataSource, FromClause, SourcePathWithSpan, WithClause},
        test_helpers::{TestParseContext, source_span, source_syntax, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_subselect(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        test_context.set_source_path_with_span(&source_span);
        let syntax = source_syntax::subselect()?;
        DataSource::from_ast_source(
            &syntax,
            &mut test_context,
            &WithClause::default(),
            &FromClause::default(),
        )?;
        Ok(())
    }
}

impl DataSource {
    pub(crate) fn from_ast_join(
        ast: &Join,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let left = Self::from_ast_in_scope(ast.left(), context, with, scope)?;
        let visible = scope.with_added_source(&left);
        let right = Self::from_ast_in_scope(ast.right(), context, with, &visible)?;
        right
            .namespace()
            .validate_against(&left.namespace(), context)?;
        let scope = visible.with_added_source(&right);
        let condition = SemScalarExpr::from_ast_optional(
            ast.condition(),
            context,
            &scope.with_grouping_placement("JOIN condition"),
        )?;
        Ok(Self::resolved_join(
            &left,
            &right,
            &SemanticJoinKind::from_ast(&ast.kind()),
            &condition,
            &ast.alias().as_ref().map(AliasSpec::from_ast),
            &JoinUsingClause::ast_using_clause(ast),
            ast.natural(),
        ))
    }
}

#[cfg(test)]
mod test_analyze_join_expr {
    use crate::{
        names::{ColumnName, SchemaName},
        sem::{
            ColumnDefinition, DataSource, FromClause, SourcePathWithSpan, TypeReference, WithClause,
        },
        test_helpers::{TestParseContext, source_span, source_syntax, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_join_expr(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        let int4_type_ref = TypeReference::from_master_type_name("int4");
        test_context.setup_type(int4_type_ref.clone());
        let columns = vec![ColumnDefinition::new(
            &Some(ColumnName::from("id")),
            Some(&int4_type_ref),
            Some(true),
            &[],
        )];
        test_context.setup_table("table1", &columns, &source_span);
        test_context.setup_table("table2", &columns, &source_span);
        test_context.setup_binary("=", "int4", "int4", "bool", true);

        let syntax = source_syntax::inner_join()?;
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        DataSource::from_ast_source(&syntax, &mut test_context, &with_clause, &from_clause)?;

        Ok(())
    }
}

impl DataSource {
    pub(crate) fn from_ast_from_group(
        ast: &FromGroup,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let Some((first, items)) = ast.items().split_first() else {
            return AnalysisError::raise_unexpected_input("empty FROM group");
        };
        let mut source = Self::from_ast_in_scope(first, context, with, scope)?;
        for item in items {
            let right = Self::from_ast_in_scope(item, context, with, scope)?;
            source = Self::resolved_join(
                &source,
                &right,
                &SemanticJoinKind::inner(),
                &None,
                &None,
                &None,
                false,
            );
        }
        if let Some(alias) = &ast.alias()
            && Dialect::from(context.metadata()) == Dialect::sqlite()
        {
            source = source.with_group_alias(&AliasSpec::from_ast(alias));
        }
        Ok(source)
    }
}

impl DataSource {
    pub(crate) fn from_ast_table_sample(
        ast: &TableSample,
        context: &mut impl ParseContext,
        with: &WithClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let source = Self::from_ast_in_scope(ast.relation(), context, with, scope)?;
        let expression_scope = scope.with_from_clause(&FromClause::default());
        let arguments = SemScalarExpr::from_ast_list(
            ast.arguments(),
            context,
            &expression_scope.with_grouping_placement("TABLESAMPLE"),
        )?;
        let repeatable = SemScalarExpr::from_ast_optional(
            ast.repeatable(),
            context,
            &expression_scope.with_grouping_placement("REPEATABLE"),
        )?;
        let sample = TableSampleSource::resolve(
            context,
            &source,
            &FullName::from_ast_name(ast.method())?,
            &arguments,
            &repeatable,
        )?;
        Ok(Self::sampled(&sample))
    }
}

impl DataSource {
    pub(crate) fn validate_ast_trigger_command(ast: &Relation) -> Result<(), ResolutionFailure> {
        if ast.name().parts().len() != 1 || ast.indexed().is_some() || ast.not_indexed() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(ast.name())?,
                "SQLite trigger DML requires an unqualified target without an index hint",
            );
        }
        Ok(())
    }
}
