use crate::sem::{AstConversion, convert};
use crate::{
    ast::{
        Dialect, Expr, Frame, FrameBound, FrameExclusion, FrameMode, Grouping, Literal, Lock,
        LockStrength, LockWait, Name, Order, QueryBody, Select, SetQuery, Window,
    },
    context::ParseContext,
    names::{ColumnName, OperatorName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnReferenceExpr, QueryFrame, QueryFrameBound,
        QueryGrouping, QueryLock, QueryName, QueryNameResolution, QueryOrder, QueryProblem,
        QueryWindow, ResolvedOperator, ResultColumnCollection, ScalarAnalysisContext,
        SemScalarExpr,
    },
};

/// SQL92 alias/ordinal references are only accepted at a clause's top level.
#[derive(Clone, Copy)]
pub(crate) enum ClauseBinding {
    Source,
    Order,
    Group,
    Compound,
}
impl ClauseBinding {
    const fn allows_output(self) -> bool {
        !matches!(self, Self::Source)
    }
    const fn input_first(self) -> bool {
        matches!(self, Self::Group)
    }
    const fn compound(self) -> bool {
        matches!(self, Self::Compound)
    }
}
impl<'ast> AstConversion<'ast, Expr> {
    pub(crate) fn resolve_clause(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if binding.allows_output() {
            if let Some(index) = self.ordinal() {
                if let Ok(index) = usize::try_from(index - 1)
                    && let Some(reference) = columns.output_reference(index)
                {
                    return Ok(reference);
                }
                context.report_analysis_problem(AnalysisProblem::query(
                    QueryProblem::position_out_of_range(index),
                ))?;
                return ColumnReferenceExpr::undefined_column_resolved(
                    context,
                    &ColumnName::from(index.to_string().as_str()),
                );
            }
            if let Some(name) = self.unqualified_column() {
                let column = ColumnName::from(name);
                if binding.input_first() && scope.has_input_column(context, &column)? {
                    return self.resolve_in(context, scope);
                }
                match columns.resolve_output(&column, context) {
                    Ok(Some(index)) => {
                        return Ok(columns
                            .output_reference(index)
                            .expect("resolved output position"));
                    }
                    Err(()) => {
                        return ColumnReferenceExpr::ambiguous_column_resolved(context, &column);
                    }
                    Ok(None) => {}
                }
            }
        }
        if binding.compound() {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_compound_order(),
            ))?;
            // Resolve against the compound's empty source scope, never an operand's.
        }
        self.resolve_in(context, scope)
    }
    pub(crate) fn ordinal(&self) -> Option<i64> {
        match self.ast {
            Expr::Literal(Literal::Integer(value)) => Some(i64::from(*value)),
            _ => None,
        }
    }
    pub(crate) fn unqualified_column(&self) -> Option<&'ast str> {
        match self.ast {
            Expr::Column(name) => convert(name).unqualified(),
            _ => None,
        }
    }
}
impl<'ast> AstConversion<'ast, Name> {
    pub(crate) fn unqualified(&self) -> Option<&'ast str> {
        match self.ast.0.as_slice() {
            [name] => Some(name),
            _ => None,
        }
    }
}
impl AstConversion<'_, Order> {
    pub(crate) fn resolve_for_query(
        values: &[Order],
        body: &QueryBody,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Vec<QueryOrder>, AnalysisError> {
        let mut orders = Vec::new();
        for order in values {
            if context.dialect() == Dialect::sqlite()
                && binding.compound()
                && let Some(index) = convert(body).match_sqlite_order(&order.expression)
            {
                let expression = columns
                    .output_reference(index)
                    .expect("compound output position");
                orders.push(QueryOrder::new(
                    expression,
                    order.descending,
                    order.nulls_first,
                    None,
                ));
            } else {
                orders.push(convert(order).resolve(context, scope, columns, binding)?);
            }
        }
        Ok(orders)
    }
    pub(crate) fn resolve_list(
        values: &[Order],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Vec<QueryOrder>, AnalysisError> {
        let mut result = Vec::new();
        for order in values {
            result.push(convert(order).resolve(context, scope, columns, binding)?);
        }
        Ok(result)
    }
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<QueryOrder, AnalysisError> {
        let expression =
            convert(&self.ast.expression).resolve_clause(context, scope, columns, binding)?;
        let using = self.resolve_using_operator(context)?;
        Ok(QueryOrder::new(
            expression,
            self.ast.descending,
            self.ast.nulls_first,
            using,
        ))
    }
    pub(crate) fn resolve_using_operator(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<ResolvedOperator>, AnalysisError> {
        let Some(name) = &self.ast.using else {
            return Ok(None);
        };
        let binding = match convert(name).operator_name() {
            Ok(operator_name) => {
                let binding = ResolvedOperator::resolve(context, &operator_name, true, true);
                Self::checked_name(context, &name.0.join("."), binding.resolution().clone())?;
                binding
            }
            Err(problem) => {
                problem.report_problem_to(context)?;
                ResolvedOperator::from_definition(context, &OperatorName::invalid(), None)
            }
        };
        Ok(Some(binding))
    }
    pub(crate) fn checked_name(
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

impl AstConversion<'_, Grouping> {
    pub(crate) fn resolve_list(
        values: &[Grouping],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<Vec<QueryGrouping>, AnalysisError> {
        let mut result = Vec::new();
        for group in values {
            result.push(convert(group).resolve(context, scope, columns)?);
        }
        Ok(result)
    }
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<QueryGrouping, AnalysisError> {
        match self.ast {
            Grouping::Expression(expression) => {
                Self::resolve_expression(expression, context, scope, columns)
            }
            Grouping::Empty => Ok(QueryGrouping::empty()),
            Grouping::Rollup(groups) => Ok(QueryGrouping::rollup(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
            Grouping::Cube(groups) => Ok(QueryGrouping::cube(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
            Grouping::Sets(groups) => Ok(QueryGrouping::sets(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
            Grouping::Simple(groups) => Ok(QueryGrouping::simple(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
        }
    }

    pub(crate) fn resolve_expression(
        expression: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<QueryGrouping, AnalysisError> {
        let scope = scope.with_grouping_placement("GROUP BY");
        let expression =
            convert(expression).resolve_clause(context, &scope, columns, ClauseBinding::Group)?;
        Ok(QueryGrouping::expression(expression))
    }
}
impl AstConversion<'_, Window> {
    pub(crate) fn resolve_over(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryWindow, AnalysisError> {
        // pg_query stores OVER name in WindowDef.name; WINDOW declarations
        // use the same parser field for a definition. Classify at this boundary.
        if let Some(name) = &self.ast.name {
            let reference = AstConversion::<Order>::checked_name(
                context,
                name,
                scope.resolve_window(name, context),
            )?;
            return Ok(QueryWindow::new(
                None,
                Some(reference),
                true,
                Vec::new(),
                Vec::new(),
                None,
            ));
        }
        self.resolve(context, scope)
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryWindow, AnalysisError> {
        let reference = if let Some(name) = &self.ast.reference {
            let outcome = scope.resolve_window(name, context);
            if context.dialect() == Dialect::postgres() || self.ast.reference_only {
                Some(AstConversion::<Order>::checked_name(
                    context, name, outcome,
                )?)
            } else {
                Some(QueryName::new(name, outcome))
            }
        } else {
            None
        };
        let partition = AstConversion::<Expr>::resolve_list(&self.ast.partition, context, scope)?;
        let order = AstConversion::<Order>::resolve_list(
            &self.ast.order,
            context,
            scope,
            &ResultColumnCollection::default(),
            ClauseBinding::Source,
        )?;
        let frame = self
            .ast
            .frame
            .as_ref()
            .map(|frame| convert(frame).resolve(context, scope))
            .transpose()?;
        Ok(QueryWindow::new(
            self.ast.name.clone(),
            reference,
            self.ast.reference_only,
            partition,
            order,
            frame,
        ))
    }
    pub(crate) fn resolve_definitions(
        values: &[Window],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryWindow>, AnalysisError> {
        let mut result = Vec::new();
        let mut names = Vec::new();
        for window in values {
            let definition_scope = if context.dialect() == Dialect::sqlite() {
                scope.with_window_names(Self::names(values))
            } else {
                scope.with_window_names(names.clone())
            };
            if let Some(name) = &window.name {
                if context.dialect() == Dialect::postgres() && names.contains(name) {
                    context.report_analysis_problem(AnalysisProblem::query(
                        QueryProblem::duplicate_window_definition(name),
                    ))?;
                }
                names.push(name.clone());
            }
            // A definition can only inherit from preceding definitions.
            result.push(convert(window).resolve(context, &definition_scope)?);
        }
        Ok(result)
    }
    pub(crate) fn names(values: &[Window]) -> Vec<String> {
        values
            .iter()
            .filter_map(|window| window.name.clone())
            .collect()
    }
}
impl AstConversion<'_, Frame> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryFrame, AnalysisError> {
        let frame_scope = scope.with_grouping_placement("window frame offset");
        Ok(QueryFrame::new(
            convert(&self.ast.mode).semantic_name(),
            convert(&self.ast.start).resolve(context, &frame_scope)?,
            self.ast
                .end
                .as_ref()
                .map(|end| convert(end).resolve(context, &frame_scope))
                .transpose()?,
            self.ast
                .exclusion
                .map(|exclusion| convert(&exclusion).semantic_name().to_owned()),
        ))
    }
}
impl AstConversion<'_, FrameMode> {
    pub(crate) const fn semantic_name(self) -> &'static str {
        match self.ast {
            FrameMode::Rows => "ROWS",
            FrameMode::Range => "RANGE",
            FrameMode::Groups => "GROUPS",
        }
    }
}
impl AstConversion<'_, FrameExclusion> {
    pub(crate) const fn semantic_name(self) -> &'static str {
        match self.ast {
            FrameExclusion::CurrentRow => "CURRENT ROW",
            FrameExclusion::Group => "GROUP",
            FrameExclusion::Ties => "TIES",
            FrameExclusion::NoOthers => "NO OTHERS",
        }
    }
}
impl AstConversion<'_, FrameBound> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryFrameBound, AnalysisError> {
        match self.ast {
            FrameBound::UnboundedPreceding => Ok(QueryFrameBound::unbounded_preceding()),
            FrameBound::Preceding(expression) => Ok(QueryFrameBound::preceding(
                convert(expression).resolve_in(context, scope)?,
            )),
            FrameBound::CurrentRow => Ok(QueryFrameBound::current_row()),
            FrameBound::Following(expression) => Ok(QueryFrameBound::following(
                convert(expression).resolve_in(context, scope)?,
            )),
            FrameBound::UnboundedFollowing => Ok(QueryFrameBound::unbounded_following()),
        }
    }
}
impl AstConversion<'_, Lock> {
    pub(crate) fn resolve_list(
        values: &[Lock],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryLock>, AnalysisError> {
        let mut result = Vec::new();
        for lock in values {
            let mut relations = Vec::new();
            for name in &lock.relations {
                let resolution = scope.resolve_lock(&name.0.join("."));
                relations.push(AstConversion::<Order>::checked_name(
                    context,
                    &name.0.join("."),
                    resolution,
                )?);
            }
            result.push(QueryLock::new(
                relations,
                lock.strength
                    .map(|strength| convert(&strength).semantic_name().to_owned()),
                lock.wait
                    .map(|wait| convert(&wait).semantic_name().to_owned()),
            ));
        }
        Ok(result)
    }
}
impl AstConversion<'_, LockStrength> {
    pub(crate) const fn semantic_name(self) -> &'static str {
        match self.ast {
            LockStrength::None => "NONE",
            LockStrength::KeyShare => "KEY SHARE",
            LockStrength::Share => "SHARE",
            LockStrength::NoKeyUpdate => "NO KEY UPDATE",
            LockStrength::Update => "UPDATE",
        }
    }
}
impl AstConversion<'_, LockWait> {
    pub(crate) const fn semantic_name(self) -> &'static str {
        match self.ast {
            LockWait::Block => "BLOCK",
            LockWait::Skip => "SKIP",
            LockWait::Error => "ERROR",
        }
    }
}

impl<'ast> AstConversion<'ast, Expr> {
    pub(crate) fn sqlite_comparison_sql(&self) -> Option<String> {
        use crate::ast::deparse::{SqlEmit, SqlWriter};
        if let Expr::Parenthesized(values) = self.ast
            && let [expression] = values.as_slice()
        {
            return convert(expression).sqlite_comparison_sql();
        }
        let mut writer = SqlWriter::new(Dialect::sqlite());
        self.ast.emit(&mut writer).ok()?;
        Some(writer.into_sql())
    }
}
impl AstConversion<'_, QueryBody> {
    pub(crate) fn match_sqlite_order(&self, expression: &Expr) -> Option<usize> {
        match self.ast {
            QueryBody::Select(select) => convert(select.as_ref()).match_sqlite_order(expression),
            QueryBody::Set(set) => convert(set.as_ref()).match_sqlite_order(expression),
            QueryBody::Values(rows) => Self::match_sqlite_values_order(rows, expression),
        }
    }
}
impl AstConversion<'_, QueryBody> {
    fn match_sqlite_values_order(rows: &[Vec<Expr>], expression: &Expr) -> Option<usize> {
        let key = convert(expression).sqlite_comparison_sql()?;
        rows.first()?
            .iter()
            .position(|value| convert(value).sqlite_comparison_sql().as_ref() == Some(&key))
    }
}

impl AstConversion<'_, SetQuery> {
    fn match_sqlite_order(&self, expression: &Expr) -> Option<usize> {
        if let Some(index) = convert(&self.ast.left.body).match_sqlite_order(expression) {
            Some(index)
        } else {
            convert(&self.ast.right.body).match_sqlite_order(expression)
        }
    }
}

impl AstConversion<'_, Select> {
    pub(crate) fn match_sqlite_order(&self, expression: &Expr) -> Option<usize> {
        let key = convert(expression).sqlite_comparison_sql()?;
        self.ast.targets.iter().position(|target| {
            convert(&target.expression).sqlite_comparison_sql().as_ref() == Some(&key)
                || convert(expression)
                    .unqualified_column()
                    .is_some_and(|name| target.alias.as_deref() == Some(name))
        })
    }
}
