use crate::{
    ast::{
        Call, Dialect, Expr, Frame, FrameBound, FrameExclusion, FrameMode, Grouping, Into, Literal,
        Lock, LockStrength, LockWait, Name, OperatorKind, Order, QueryBody, Select, SetQuery,
        Window,
    },
    context::ParseContext,
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, AnalysisProblem, ArithExpr, FunctionModifiers, QueryFrame, QueryFrameBound,
        QueryGrouping, QueryLock, QueryName, QueryNameResolution, QueryOrder, QueryProblem,
        QueryWindow, ResolvedOperator, ResultColumnCollection, ScalarAnalysisContext,
        SelectDestination, SemScalarExpr,
    },
};

impl ResolvedOperator {
    pub(crate) fn from_ast_operator_name(
        ast: &OperatorKind,
        context: &impl ParseContext,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<Self> {
        if ArithExpr::ast_has_named_operator(ast) {
            Some(Self::resolve(context, name, left, right))
        } else {
            None
        }
    }
}

impl QueryOrder {
    pub(crate) const fn ast_order_binding(ast: &QueryBody) -> ClauseBinding {
        match ast {
            QueryBody::Set(_) => ClauseBinding::Compound,
            _ => ClauseBinding::Order,
        }
    }
}

impl FunctionModifiers {
    pub(crate) fn from_ast_modifiers(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let columns = ResultColumnCollection::default();
        let order = QueryOrder::from_ast_list(
            ast.order(),
            context,
            scope,
            &columns,
            ClauseBinding::Source,
        )?;
        let within_order = QueryOrder::from_ast_list(
            ast.within_order(),
            context,
            scope,
            &columns,
            ClauseBinding::Source,
        )?;
        let filter = SemScalarExpr::from_ast_optional(
            ast.filter(),
            context,
            &scope.with_grouping_placement("aggregate FILTER"),
        )?;
        let over = ast
            .over()
            .as_ref()
            .map(|window| QueryWindow::from_ast_over(window, context, scope))
            .transpose()?;
        let (within_order, order) = if ast.within_group() && within_order.is_empty() {
            (order, Vec::new())
        } else {
            (within_order, order)
        };
        let mut modifiers = Self::new(within_order, order, filter, over);
        modifiers.set_flags(
            ast.distinct(),
            ast.star(),
            ast.variadic(),
            ast.within_group(),
        );
        Ok(modifiers)
    }
}

impl SelectDestination {
    pub(crate) fn from_ast_destination(ast: &Into) -> Result<Self, AnalysisError> {
        Ok(Self::new(
            &FullName::from_ast_name(ast.relation().name())?,
            ast.relation().persistence(),
        ))
    }
}

/// SQL92 alias/ordinal references are only accepted at a clause's top level.
#[derive(Clone, Copy)]
pub(crate) enum ClauseBinding {
    Source,
    Order,
    Group,
    Compound,
}

impl ClauseBinding {
    pub(crate) const fn allows_output(self) -> bool {
        !matches!(self, Self::Source)
    }
    pub(crate) const fn input_first(self) -> bool {
        matches!(self, Self::Group)
    }
    pub(crate) const fn compound(self) -> bool {
        matches!(self, Self::Compound)
    }
}

impl QueryOrder {
    pub(crate) fn ast_ordinal(ast: &Expr) -> Option<i64> {
        match ast {
            Expr::Literal(Literal::Integer(value)) => Some(i64::from(*value)),
            _ => None,
        }
    }
}

impl QueryOrder {
    pub(crate) fn ast_unqualified_column(ast: &Expr) -> Option<&str> {
        match ast {
            Expr::Column(name) => Self::ast_unqualified_name(name),
            _ => None,
        }
    }
}

impl QueryOrder {
    pub(crate) fn ast_unqualified_name(ast: &Name) -> Option<&str> {
        match ast.parts() {
            [name] => Some(name),
            _ => None,
        }
    }
}

impl QueryOrder {
    pub(crate) fn from_ast_for_query(
        values: &[Order],
        body: &QueryBody,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut orders = Vec::new();
        for order in values {
            if context.dialect() == Dialect::sqlite()
                && binding.compound()
                && let Some(index) = Self::match_ast_compound_order(body, order.expression())
            {
                let expression = columns
                    .output_reference(index)
                    .expect("compound output position");
                orders.push(Self::new(
                    expression,
                    order.descending(),
                    order.nulls_first(),
                    None,
                ));
            } else {
                orders.push(Self::from_ast(order, context, scope, columns, binding)?);
            }
        }
        Ok(orders)
    }
}

impl QueryOrder {
    pub(crate) fn from_ast_list(
        values: &[Order],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut result = Vec::new();
        for order in values {
            result.push(Self::from_ast(order, context, scope, columns, binding)?);
        }
        Ok(result)
    }
}

impl QueryOrder {
    pub(crate) fn from_ast(
        ast: &Order,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Self, AnalysisError> {
        let expression =
            SemScalarExpr::from_ast_clause(ast.expression(), context, scope, columns, binding)?;
        let using = Self::from_ast_using_operator(ast, context)?;
        Ok(Self::new(
            expression,
            ast.descending(),
            ast.nulls_first(),
            using,
        ))
    }
}

impl QueryOrder {
    pub(crate) fn from_ast_using_operator(
        ast: &Order,
        context: &mut impl ParseContext,
    ) -> Result<Option<ResolvedOperator>, AnalysisError> {
        let Some(name) = &ast.using() else {
            return Ok(None);
        };
        let binding = match OperatorName::from_ast_name(name) {
            Ok(operator_name) => {
                let binding = ResolvedOperator::resolve(context, &operator_name, true, true);
                Self::ast_checked_name(
                    context,
                    &name.parts().join("."),
                    binding.resolution().clone(),
                )?;
                binding
            }
            Err(problem) => {
                problem.report_problem_to(context)?;
                ResolvedOperator::from_definition(context, &OperatorName::invalid(), None)
            }
        };
        Ok(Some(binding))
    }
}

#[cfg(test)]
mod ordering_operator_tests {
    use crate::{
        sem::QueryNameResolution,
        test_helpers::operator_names::{addition_context, analyze},
    };

    #[test]
    fn cross_database_ordering_operator_retains_failed_binding() -> testresult::TestResult {
        let (query, context) = analyze(
            "SELECT 1 + 2 ORDER BY 1 USING OPERATOR(a.b.<)",
            addition_context(None),
        )?;
        assert_eq!(context.reported_problem_count(), 1);
        assert!(context.has_reported_problem("cross_database_operator_reference"));
        assert_eq!(
            query.query_clauses().unwrap().order()[0]
                .using()
                .unwrap()
                .resolution(),
            &QueryNameResolution::not_found()
        );
        Ok(())
    }
}

impl QueryOrder {
    pub(crate) fn ast_checked_name(
        context: &mut impl ParseContext,
        name: &str,
        resolution: QueryNameResolution,
    ) -> Result<QueryName, AnalysisError> {
        if let Some(problem) = resolution.reference_problem(name) {
            context.report_analysis_problem(AnalysisProblem::query(problem))?;
        }
        Ok(QueryName::new(name, resolution))
    }
}

impl QueryGrouping {
    pub(crate) fn from_ast_list(
        values: &[Grouping],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut result = Vec::new();
        for group in values {
            result.push(Self::from_ast(group, context, scope, columns)?);
        }
        Ok(result)
    }
}

impl QueryGrouping {
    pub(crate) fn from_ast(
        ast: &Grouping,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Grouping::Expression(expression) => {
                Self::from_ast_expression(expression, context, scope, columns)
            }
            Grouping::Empty => Ok(Self::empty()),
            Grouping::Rollup(groups) => Ok(Self::rollup(Self::from_ast_list(
                groups, context, scope, columns,
            )?)),
            Grouping::Cube(groups) => Ok(Self::cube(Self::from_ast_list(
                groups, context, scope, columns,
            )?)),
            Grouping::Sets(groups) => Ok(Self::sets(Self::from_ast_list(
                groups, context, scope, columns,
            )?)),
            Grouping::Simple(groups) => Ok(Self::simple(Self::from_ast_list(
                groups, context, scope, columns,
            )?)),
        }
    }
}

impl QueryGrouping {
    pub(crate) fn from_ast_expression(
        expression: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<Self, AnalysisError> {
        let scope = scope.with_grouping_placement("GROUP BY");
        let expression = SemScalarExpr::from_ast_clause(
            expression,
            context,
            &scope,
            columns,
            ClauseBinding::Group,
        )?;
        Ok(Self::expression(expression))
    }
}

impl QueryWindow {
    pub(crate) fn from_ast_over(
        ast: &Window,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        // An OVER reference stores its name in WindowDef.name; WINDOW declarations
        // use the same parser field for a definition. Classify at this boundary.
        if let Some(name) = &ast.name() {
            let reference =
                QueryOrder::ast_checked_name(context, name, scope.resolve_window(name, context))?;
            return Ok(Self::new(
                None,
                Some(reference),
                true,
                Vec::new(),
                Vec::new(),
                None,
            ));
        }
        Self::from_ast(ast, context, scope)
    }
}

impl QueryWindow {
    pub(crate) fn from_ast(
        ast: &Window,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let reference = if let Some(name) = &ast.reference() {
            let outcome = scope.resolve_window(name, context);
            if context.dialect() == Dialect::postgres() || ast.reference_only() {
                Some(QueryOrder::ast_checked_name(context, name, outcome)?)
            } else {
                Some(QueryName::new(name, outcome))
            }
        } else {
            None
        };
        let partition = SemScalarExpr::from_ast_list(ast.partition(), context, scope)?;
        let order = QueryOrder::from_ast_list(
            ast.order(),
            context,
            scope,
            &ResultColumnCollection::default(),
            ClauseBinding::Source,
        )?;
        let frame = ast
            .frame()
            .as_ref()
            .map(|frame| QueryFrame::from_ast(frame, context, scope))
            .transpose()?;
        Ok(Self::new(
            ast.name().clone(),
            reference,
            ast.reference_only(),
            partition,
            order,
            frame,
        ))
    }
}

#[cfg(test)]
mod test_missing_window_inheritance_is_an_explicit_failed_lookup {
    #[cfg(feature = "sqlite")]
    use crate::sem::{ColumnReferenceExpr, SemScalarExpr};

    use crate::{ast::Dialect, sem::QueryNameResolution, test_helpers::query::resolve};

    #[test]
    fn missing_window_inheritance_is_an_explicit_failed_lookup() {
        let (query, context) = resolve(
            "SELECT sum(1) OVER w WINDOW w AS (missing)",
            Dialect::postgres(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        let window = &query
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap()
            .windows()[0];
        assert_matches::assert_matches!(
            window.reference().unwrap().resolution(),
            QueryNameResolution::NotFound
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_window_definitions_do_not_bind_projection_aliases() {
        let (query, context) = resolve(
            "SELECT 1 AS n WINDOW w AS (ORDER BY n)",
            Dialect::sqlite(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        let windows = query
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap()
            .windows();
        assert_matches::assert_matches!(
            windows[0].order()[0].expression(),
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_))
        );
    }
}

impl QueryWindow {
    pub(crate) fn from_ast_definitions(
        values: &[Window],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut result = Vec::new();
        let mut names = Vec::new();
        for window in values {
            let definition_scope = if context.dialect() == Dialect::sqlite() {
                scope.with_window_names(Self::ast_names(values))
            } else {
                scope.with_window_names(names.clone())
            };
            if let Some(name) = &window.name() {
                if context.dialect() == Dialect::postgres() && names.contains(name) {
                    context.report_analysis_problem(AnalysisProblem::query(
                        QueryProblem::duplicate_window_definition(name),
                    ))?;
                }
                names.push(name.clone());
            }
            // A definition can only inherit from preceding definitions.
            result.push(Self::from_ast(window, context, &definition_scope)?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod test_sqlite_window_inheritance_records_dialect_binding {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, sem::QueryNameResolution, test_helpers::query::resolve};

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::forward(
        "SELECT 1 WINDOW w AS (later), later AS ()",
        QueryNameResolution::WindowAlias(1)
    )]
    #[case::missing_base("SELECT 1 WINDOW w AS (missing)", QueryNameResolution::NotFound)]
    fn sqlite_window_inheritance_records_dialect_binding(
        #[case] sql: &str,
        #[case] expected: QueryNameResolution,
    ) {
        let connection = ::rusqlite::Connection::open_in_memory().unwrap();
        connection.prepare(sql).unwrap();
        let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let clauses = query
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap();
        assert_eq!(
            clauses.windows()[0].reference().unwrap().resolution(),
            &expected
        );
    }
}

impl QueryWindow {
    pub(crate) fn ast_names(values: &[Window]) -> Vec<String> {
        values
            .iter()
            .filter_map(|window| window.name().clone())
            .collect()
    }
}

impl QueryFrame {
    pub(crate) fn from_ast(
        ast: &Frame,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let frame_scope = scope.with_grouping_placement("window frame offset");
        Ok(Self::new(
            Self::ast_mode_name(&ast.mode()),
            QueryFrameBound::from_ast(ast.start(), context, &frame_scope)?,
            ast.end()
                .as_ref()
                .map(|end| QueryFrameBound::from_ast(end, context, &frame_scope))
                .transpose()?,
            ast.exclusion()
                .map(|exclusion| Self::ast_exclusion_name(&exclusion).to_owned()),
        ))
    }
}

impl QueryFrame {
    pub(crate) const fn ast_mode_name(ast: &FrameMode) -> &'static str {
        match ast {
            FrameMode::Rows => "ROWS",
            FrameMode::Range => "RANGE",
            FrameMode::Groups => "GROUPS",
        }
    }
}

impl QueryFrame {
    pub(crate) const fn ast_exclusion_name(ast: &FrameExclusion) -> &'static str {
        match ast {
            FrameExclusion::CurrentRow => "CURRENT ROW",
            FrameExclusion::Group => "GROUP",
            FrameExclusion::Ties => "TIES",
            FrameExclusion::NoOthers => "NO OTHERS",
        }
    }
}

impl QueryFrameBound {
    pub(crate) fn from_ast(
        ast: &FrameBound,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            FrameBound::UnboundedPreceding => Ok(Self::unbounded_preceding()),
            FrameBound::Preceding(expression) => Ok(Self::preceding(SemScalarExpr::from_ast_in(
                expression, context, scope,
            )?)),
            FrameBound::CurrentRow => Ok(Self::current_row()),
            FrameBound::Following(expression) => Ok(Self::following(SemScalarExpr::from_ast_in(
                expression, context, scope,
            )?)),
            FrameBound::UnboundedFollowing => Ok(Self::unbounded_following()),
        }
    }
}

impl QueryLock {
    pub(crate) fn from_ast_list(
        values: &[Lock],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut result = Vec::new();
        for lock in values {
            let mut relations = Vec::new();
            for name in lock.relations() {
                let resolution = scope.resolve_lock(&name.parts().join("."));
                relations.push(QueryOrder::ast_checked_name(
                    context,
                    &name.parts().join("."),
                    resolution,
                )?);
            }
            result.push(Self::new(
                relations,
                lock.strength()
                    .map(|strength| Self::ast_strength_name(&strength).to_owned()),
                lock.wait()
                    .map(|wait| Self::ast_wait_name(&wait).to_owned()),
            ));
        }
        Ok(result)
    }
}

impl QueryLock {
    pub(crate) const fn ast_strength_name(ast: &LockStrength) -> &'static str {
        match ast {
            LockStrength::None => "NONE",
            LockStrength::KeyShare => "KEY SHARE",
            LockStrength::Share => "SHARE",
            LockStrength::NoKeyUpdate => "NO KEY UPDATE",
            LockStrength::Update => "UPDATE",
        }
    }
}

impl QueryLock {
    pub(crate) const fn ast_wait_name(ast: &LockWait) -> &'static str {
        match ast {
            LockWait::Block => "BLOCK",
            LockWait::Skip => "SKIP",
            LockWait::Error => "ERROR",
        }
    }
}

impl QueryOrder {
    pub(crate) fn ast_sqlite_comparison_sql(ast: &Expr) -> Option<String> {
        use crate::ast::deparse::{SqlEmit, SqlWriter};
        if let Expr::Parenthesized(values) = ast
            && let [expression] = values.as_slice()
        {
            return Self::ast_sqlite_comparison_sql(expression);
        }
        let mut writer = SqlWriter::new(Dialect::sqlite());
        ast.emit(&mut writer).ok()?;
        Some(writer.into_sql())
    }
}

impl QueryOrder {
    pub(crate) fn match_ast_compound_order(ast: &QueryBody, expression: &Expr) -> Option<usize> {
        match ast {
            QueryBody::Select(select) => Self::match_ast_select_order(select.as_ref(), expression),
            QueryBody::Set(set) => Self::match_ast_set_order(set.as_ref(), expression),
            QueryBody::Values(rows) => Self::match_ast_values_order(rows, expression),
        }
    }
}

#[cfg(test)]
mod test_sqlite_binds_ordering_to_its_output {
    #[cfg(feature = "sqlite")]
    use crate::{
        ast::Dialect,
        test_helpers::query::{output_index, resolve},
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::right_alias("SELECT 1 AS n UNION ALL SELECT 2 AS m ORDER BY m")]
    #[case::right_expression(
        "SELECT CAST(1 AS TEXT) UNION ALL SELECT CAST(2 AS TEXT) ORDER BY CAST(2 AS TEXT)"
    )]
    #[case::duplicate_alias("SELECT 1 AS n, 2 AS n ORDER BY n")]
    fn sqlite_binds_ordering_to_its_output(#[case] sql: &str) {
        let connection = ::rusqlite::Connection::open_in_memory().unwrap();
        connection.prepare(sql).unwrap();
        let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(
            output_index(query.query_clauses().unwrap().order()[0].expression()),
            Some(0)
        );
    }
}

impl QueryOrder {
    fn match_ast_values_order(rows: &[Vec<Expr>], expression: &Expr) -> Option<usize> {
        let key = Self::ast_sqlite_comparison_sql(expression)?;
        rows.first()?
            .iter()
            .position(|value| Self::ast_sqlite_comparison_sql(value).as_ref() == Some(&key))
    }
}

impl QueryOrder {
    fn match_ast_set_order(ast: &SetQuery, expression: &Expr) -> Option<usize> {
        if let Some(index) = Self::match_ast_compound_order(ast.left().body(), expression) {
            Some(index)
        } else {
            Self::match_ast_compound_order(ast.right().body(), expression)
        }
    }
}

impl QueryOrder {
    pub(crate) fn match_ast_select_order(ast: &Select, expression: &Expr) -> Option<usize> {
        let key = Self::ast_sqlite_comparison_sql(expression)?;
        ast.targets().iter().position(|target| {
            Self::ast_sqlite_comparison_sql(target.expression()).as_ref() == Some(&key)
                || Self::ast_unqualified_column(expression)
                    .is_some_and(|name| target.alias().as_deref() == Some(name))
        })
    }
}
