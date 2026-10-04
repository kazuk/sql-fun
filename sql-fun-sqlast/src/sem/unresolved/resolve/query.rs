use crate::sem::ColumnName;
use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, CteStatement, DataSource, FromClause,
    JoinKind as SemanticJoinKind, ScalarAnalysisContext, SelectStatement, WithClause,
    resolution::{Catalog, Resolution, ResolutionContext, ResolutionOutcome},
    select_statement::{ResultColumnCollection, SelectStatementNode},
    unresolved::{
        Alias, Call, CallKind, CteBody, Dialect, Expr, FromItem, Join, JoinKind, Query, QueryBody,
        Select, SetOperation, TableFunctions, With,
    },
};

impl Query {
    pub(crate) fn resolve(
        &self,
        catalog: &dyn Catalog,
        dialect: Dialect,
    ) -> ResolutionOutcome<SelectStatement> {
        let mut context = Resolution::new(catalog);
        let result = self.resolve_in(&mut context, dialect, None);
        context.finish(result)
    }

    pub(super) fn resolve_in(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        crate::sem::stack::with_stack(|| Self::resolve_in_body(self, context, dialect, outer))
    }

    #[inline(never)]
    fn resolve_in_body(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        let with = self.with.resolve(context, dialect, outer)?;
        self.body.resolve(context, dialect, &with, outer)
    }
}

impl QueryBody {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        match self {
            Self::Select(select) => select.resolve(context, dialect, with, outer),
            Self::Set(set) => {
                let from = FromClause::default();
                let scope = ScalarAnalysisContext::nested(with, &from, outer);
                let left = set.left.resolve_in(context, dialect, Some(&scope))?;
                let right = set.right.resolve_in(context, dialect, Some(&scope))?;
                Ok(set.operation.resolve(left, right))
            }
            // Legacy SELECT analysis returns no inferred columns for VALUES.
            // Keep that semantic limitation while retaining every row in syntax.
            Self::Values(_) => Ok(SelectStatement::simple(&SelectStatementNode::new(
                &ResultColumnCollection::default(),
                with,
                &FromClause::default(),
                &None,
            ))),
        }
    }
}

impl SetOperation {
    fn resolve(&self, left: SelectStatement, right: SelectStatement) -> SelectStatement {
        match self {
            Self::Union => SelectStatement::union(left, right),
            Self::Intersect => SelectStatement::intersect(left, right),
            Self::Except => SelectStatement::except(left, right),
        }
    }
}

impl Select {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<SelectStatement, AnalysisError> {
        let from = FromItem::resolve_list_in(&self.from, context, dialect, with, outer)?;
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        let predicate = Expr::resolve_optional(&self.predicate, context, dialect, &scope)?;
        let mut results = ResultColumnCollection::default();
        for target in &self.targets {
            let expression = target.expression.resolve_in(context, dialect, &scope)?;
            let name = target
                .alias
                .clone()
                .unwrap_or_else(|| expression.get_column_name());
            results.push_column(&ColumnName::from(name.as_str()), &expression);
        }
        Ok(SelectStatement::simple(&SelectStatementNode::new(
            &results, with, &from, &predicate,
        )))
    }
}

impl With {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<WithClause, AnalysisError> {
        let mut with = outer
            .map(ScalarAnalysisContext::inherited_ctes)
            .unwrap_or_default();
        for entry in &self.entries {
            let from = FromClause::default();
            let scope = ScalarAnalysisContext::nested(&with, &from, outer);
            let body = entry
                .body
                .resolve(context, dialect, &scope, &entry.column_names())?;
            with.insert(&entry.name, body);
        }
        Ok(with)
    }
}

impl CteBody {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
    ) -> Result<CteStatement, AnalysisError> {
        match self {
            Self::Query(query) => {
                let mut query = query.resolve_in(context, dialect, Some(scope))?;
                query.rename_columns(columns);
                Ok(CteStatement::from_query(&query))
            }
            // Preserve the existing semantic DML representation. Construction has
            // already retained the complete recursive syntax, without parser nodes.
            Self::Insert(_) => Ok(CteStatement::insert()),
            Self::Update(_) => Ok(CteStatement::update()),
            Self::Delete(_) => Ok(CteStatement::delete()),
            Self::Merge(_) => AnalysisError::raise_unexpected_none("common_table_expr.query"),
        }
    }
}

impl Alias {
    fn resolve(&self) -> AliasSpec {
        AliasSpec::from_names(&self.name, &self.columns)
    }
}

impl JoinKind {
    const fn resolve(&self) -> SemanticJoinKind {
        match self {
            Self::Inner | Self::Cross => SemanticJoinKind::inner(),
            Self::Left => SemanticJoinKind::left(),
            Self::Right => SemanticJoinKind::right(),
            Self::Full => SemanticJoinKind::full(),
        }
    }
}

impl FromItem {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        with: &WithClause,
        preceding: &FromClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<DataSource, AnalysisError> {
        crate::sem::stack::with_stack(|| {
            Self::resolve_body(self, context, dialect, with, preceding, outer)
        })
    }

    #[inline(never)]
    fn resolve_body(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        with: &WithClause,
        preceding: &FromClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<DataSource, AnalysisError> {
        match self {
            Self::Relation(relation) => {
                let name = relation.name.full_name()?;
                let alias = relation
                    .alias
                    .as_ref()
                    .map(Alias::resolve)
                    .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
                DataSource::resolve_relation(context, &name, &alias, with)
            }
            Self::Query(derived) => {
                let from = if derived.lateral {
                    preceding.clone()
                } else {
                    FromClause::default()
                };
                let scope = ScalarAnalysisContext::nested(with, &from, outer);
                let query = derived.query.resolve_in(context, dialect, Some(&scope))?;
                let alias = derived
                    .alias
                    .as_ref()
                    .map(Alias::resolve)
                    .unwrap_or_else(|| AliasSpec::from_local_name(""));
                Ok(DataSource::resolved_query(&query, &alias))
            }
            Self::Join(join) => {
                let left = join
                    .left
                    .resolve(context, dialect, with, preceding, outer)?;
                let mut visible = preceding.clone();
                visible.push(left.clone());
                let right = join
                    .right
                    .resolve(context, dialect, with, &visible, outer)?;
                visible.push(right.clone());
                let scope = ScalarAnalysisContext::nested(with, &visible, outer);
                let condition = Expr::resolve_optional(&join.condition, context, dialect, &scope)?;
                Ok(DataSource::resolved_join(
                    &left,
                    &right,
                    &join.kind.resolve(),
                    &condition,
                    &join.alias.as_ref().map(Alias::resolve),
                    &join.using_clause(),
                    join.natural,
                ))
            }
            Self::Functions(functions) => {
                functions.resolve(context, dialect, with, preceding, outer)
            }
            Self::Group(group) => {
                let Some((first, items)) = group.items.split_first() else {
                    return AnalysisError::raise_unexpected_input("empty FROM group");
                };
                let mut source = first.resolve(context, dialect, with, preceding, outer)?;
                for item in items {
                    let right = item.resolve(context, dialect, with, preceding, outer)?;
                    source = DataSource::resolved_join(
                        &source,
                        &right,
                        &SemanticJoinKind::inner(),
                        &None,
                        &None,
                        &None,
                        false,
                    );
                }
                if let Some(alias) = &group.alias
                    && Dialect::from(context.metadata()) == Dialect::sqlite()
                {
                    source = source.with_group_alias(&alias.resolve());
                }
                Ok(source)
            }
            Self::Xml(table) => table.resolve(
                context,
                dialect,
                &ScalarAnalysisContext::nested(with, preceding, outer),
            ),
            Self::Sample(_) => todo!("table expression semantic analysis"),
            Self::Json(_) => AnalysisError::raise_unexpected_none("from_clause inner node"),
        }
    }
}

impl Join {
    fn using_clause(&self) -> Option<crate::sem::data_source::JoinUsingClause> {
        if self.using.is_empty() {
            return None;
        }
        let mut columns = Vec::new();
        for column in &self.using {
            columns.push(ColumnName::from(column.as_str()));
        }
        Some(crate::sem::data_source::JoinUsingClause::new(
            &columns,
            &self.using_alias.as_ref().map(Alias::resolve),
        ))
    }
}

impl TableFunctions {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        with: &WithClause,
        preceding: &FromClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<DataSource, AnalysisError> {
        let scope = ScalarAnalysisContext::nested(with, preceding, outer);
        if self.rows_from {
            let mut functions = Vec::new();
            for function in &self.calls {
                functions.push(
                    function
                        .expression
                        .resolve_rows_call(context, dialect, &scope)?,
                );
            }
            let alias = self
                .alias
                .as_ref()
                .map(Alias::resolve)
                .unwrap_or_else(|| AliasSpec::from_local_name(""));
            return Ok(DataSource::resolved_rows(&functions, &alias));
        }
        let [function] = self.calls.as_slice() else {
            return AnalysisError::raise_unexpected_input("table function count");
        };
        function
            .expression
            .resolve_table_call(context, dialect, &scope, self.alias.as_ref())
    }
}

impl Expr {
    pub(crate) fn resolve_rows(
        &self,
        catalog: &dyn Catalog,
        scope: &ScalarAnalysisContext<'_>,
    ) -> ResolutionOutcome<crate::sem::DataSourceRowsFromFunc> {
        let mut context = Resolution::new(catalog);
        let result = self.resolve_rows_call(&mut context, Dialect::postgres(), scope);
        context.finish(result)
    }
    fn resolve_rows_call(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<crate::sem::DataSourceRowsFromFunc, AnalysisError> {
        match self {
            Self::Call(call) => {
                call.kind.require_table_function()?;
                let arguments = Self::resolve_list(&call.arguments, context, dialect, scope)?;
                crate::sem::DataSourceRowsFromFunc::resolve(
                    context,
                    &call.name.full_name()?,
                    &arguments,
                )
            }
            Self::SqlValue(_) => todo!("ROWS FROM expression analysis"),
            _ => AnalysisError::raise_unexpected_input("ROWS FROM expression"),
        }
    }
    pub(in crate::sem::unresolved) fn resolve_table_call(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&Alias>,
    ) -> Result<DataSource, AnalysisError> {
        match self {
            Self::Call(call) => call.resolve_table(context, dialect, scope, alias),
            _ => AnalysisError::raise_unexpected_none("table expression is not FuncCall"),
        }
    }
}

impl Call {
    fn resolve_table(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
        alias: Option<&Alias>,
    ) -> Result<DataSource, AnalysisError> {
        if dialect == Dialect::sqlite() {
            todo!("SQLite table function affinity and overload semantics");
        }
        self.kind.require_table_function()?;
        let arguments = Expr::resolve_list(&self.arguments, context, dialect, scope)?;
        let name = self.name.full_name()?;
        let alias = alias
            .map(Alias::resolve)
            .unwrap_or_else(|| AliasSpec::from_local_name(&name.local_name()));
        DataSource::resolve_function(context, &name, &arguments, &alias)
    }
}

impl CallKind {
    fn require_table_function(&self) -> Result<(), AnalysisError> {
        match self {
            Self::Ordinary => Ok(()),
            // The old FROM boundary accepted FuncCall, not special scalar forms.
            Self::Coalesce | Self::MinMax => {
                AnalysisError::raise_unexpected_input("table expression is not FuncCall")
            }
        }
    }
}

impl FromItem {
    fn resolve_list_in(
        items: &[Self],
        context: &mut Resolution<'_>,
        dialect: Dialect,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        let mut from = FromClause::default();
        let mut aliases = Vec::new();
        for item in items {
            let source = item.resolve(context, dialect, with, &from, outer)?;
            for alias in source.get_aliases() {
                if aliases.contains(&alias) {
                    context.report_problem(AnalysisProblem::duplicate_alias(alias.name()))?;
                }
                aliases.push(alias);
            }
            from.push(source);
        }

        Ok(from)
    }
    pub(crate) fn resolve_list(
        items: &[Self],
        catalog: &dyn Catalog,
        dialect: Dialect,
        with: &WithClause,
    ) -> ResolutionOutcome<FromClause> {
        let mut context = Resolution::new(catalog);
        let result = Self::resolve_list_in(items, &mut context, dialect, with, None);
        context.finish(result)
    }
    pub(crate) fn resolve_source(
        &self,
        catalog: &dyn Catalog,
        dialect: Dialect,
        with: &WithClause,
        preceding: &FromClause,
    ) -> ResolutionOutcome<DataSource> {
        let mut context = Resolution::new(catalog);
        let result = self.resolve(&mut context, dialect, with, preceding, None);
        context.finish(result)
    }
}
impl With {
    pub(crate) fn resolve_with(
        &self,
        catalog: &dyn Catalog,
        dialect: Dialect,
    ) -> ResolutionOutcome<WithClause> {
        let mut context = Resolution::new(catalog);
        let result = self.resolve(&mut context, dialect, None);
        context.finish(result)
    }
}
