use crate::ast::{
    Dialect, Expr, Frame, FrameBound, FrameExclusion, FrameMode, Grouping, Literal, Lock,
    LockStrength, LockWait, Name, Order, Window,
};
use crate::sem::{
    AnalysisError, AnalysisProblem, ColumnName, ColumnReferenceExpr, ParseContext, QueryFrame,
    QueryFrameBound, QueryGrouping, QueryLock, QueryName, QueryNameResolution, QueryOrder,
    QueryProblem, QueryWindow, ResultColumnCollection, ScalarAnalysisContext, SemScalarExpr,
};

/// SQL92 alias/ordinal references are only accepted at a clause's top level.
#[derive(Clone, Copy)]
pub(super) enum ClauseBinding {
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
impl Expr {
    pub(super) fn resolve_clause(
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
                if binding.input_first() && scope.has_input_column(context, &column) {
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
    fn ordinal(&self) -> Option<i64> {
        match self {
            Self::Literal(Literal::Integer(value)) => Some(i64::from(*value)),
            _ => None,
        }
    }
    fn unqualified_column(&self) -> Option<&str> {
        match self {
            Self::Column(name) => name.unqualified(),
            _ => None,
        }
    }
}
impl Name {
    fn unqualified(&self) -> Option<&str> {
        match self.0.as_slice() {
            [name] => Some(name),
            _ => None,
        }
    }
}
impl Order {
    pub(super) fn resolve_for_query(
        values: &[Self],
        body: &crate::ast::QueryBody,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Vec<QueryOrder>, AnalysisError> {
        let mut orders = Vec::new();
        for order in values {
            if context.dialect() == Dialect::sqlite()
                && binding.compound()
                && let Some(index) = body.match_sqlite_order(&order.expression)
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
                orders.push(order.resolve(context, scope, columns, binding)?);
            }
        }
        Ok(orders)
    }
    pub(super) fn resolve_list(
        values: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Vec<QueryOrder>, AnalysisError> {
        let mut result = Vec::new();
        for order in values {
            result.push(order.resolve(context, scope, columns, binding)?);
        }
        Ok(result)
    }
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<QueryOrder, AnalysisError> {
        let expression = self
            .expression
            .resolve_clause(context, scope, columns, binding)?;
        let using = if let Some(name) = &self.using {
            let operator_name = name.operator_name()?;
            let binding =
                crate::sem::ResolvedOperator::resolve(context, &operator_name, true, true);
            Self::checked_name(context, &name.0.join("."), binding.resolution().clone())?;
            Some(binding)
        } else {
            None
        };
        Ok(QueryOrder::new(
            expression,
            self.descending,
            self.nulls_first,
            using,
        ))
    }
    fn checked_name(
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
impl Grouping {
    pub(super) fn resolve_list(
        values: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<Vec<QueryGrouping>, AnalysisError> {
        let mut result = Vec::new();
        for group in values {
            result.push(group.resolve(context, scope, columns)?);
        }
        Ok(result)
    }
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
    ) -> Result<QueryGrouping, AnalysisError> {
        match self {
            Self::Expression(expression) => {
                Ok(QueryGrouping::expression(expression.resolve_clause(
                    context,
                    &scope.with_grouping_placement("GROUP BY"),
                    columns,
                    ClauseBinding::Group,
                )?))
            }
            Self::Empty => Ok(QueryGrouping::empty()),
            Self::Rollup(groups) => Ok(QueryGrouping::rollup(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
            Self::Cube(groups) => Ok(QueryGrouping::cube(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
            Self::Sets(groups) => Ok(QueryGrouping::sets(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
            Self::Simple(groups) => Ok(QueryGrouping::simple(Self::resolve_list(
                groups, context, scope, columns,
            )?)),
        }
    }
}
impl Window {
    pub(super) fn resolve_over(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryWindow, AnalysisError> {
        // pg_query stores OVER name in WindowDef.name; WINDOW declarations
        // use the same parser field for a definition. Classify at this boundary.
        if let Some(name) = &self.name {
            let reference =
                Order::checked_name(context, name, scope.resolve_window(name, context))?;
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

    pub(super) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryWindow, AnalysisError> {
        let reference = if let Some(name) = &self.reference {
            let outcome = scope.resolve_window(name, context);
            if context.dialect() == Dialect::postgres() || self.reference_only {
                Some(Order::checked_name(context, name, outcome)?)
            } else {
                Some(QueryName::new(name, outcome))
            }
        } else {
            None
        };
        let partition = Expr::resolve_list(&self.partition, context, scope)?;
        let order = Order::resolve_list(
            &self.order,
            context,
            scope,
            &ResultColumnCollection::default(),
            ClauseBinding::Source,
        )?;
        let frame = self
            .frame
            .as_ref()
            .map(|frame| frame.resolve(context, scope))
            .transpose()?;
        Ok(QueryWindow::new(
            self.name.clone(),
            reference,
            self.reference_only,
            partition,
            order,
            frame,
        ))
    }
    pub(super) fn resolve_definitions(
        values: &[Self],
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
            result.push(window.resolve(context, &definition_scope)?);
        }
        Ok(result)
    }
    pub(super) fn names(values: &[Self]) -> Vec<String> {
        values
            .iter()
            .filter_map(|window| window.name.clone())
            .collect()
    }
}
impl Frame {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryFrame, AnalysisError> {
        let frame_scope = scope.with_grouping_placement("window frame offset");
        Ok(QueryFrame::new(
            self.mode.semantic_name(),
            self.start.resolve(context, &frame_scope)?,
            self.end
                .as_ref()
                .map(|end| end.resolve(context, &frame_scope))
                .transpose()?,
            self.exclusion
                .map(|exclusion| exclusion.semantic_name().to_owned()),
        ))
    }
}
impl FrameMode {
    const fn semantic_name(self) -> &'static str {
        match self {
            Self::Rows => "ROWS",
            Self::Range => "RANGE",
            Self::Groups => "GROUPS",
        }
    }
}
impl FrameExclusion {
    const fn semantic_name(self) -> &'static str {
        match self {
            Self::CurrentRow => "CURRENT ROW",
            Self::Group => "GROUP",
            Self::Ties => "TIES",
            Self::NoOthers => "NO OTHERS",
        }
    }
}
impl FrameBound {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<QueryFrameBound, AnalysisError> {
        match self {
            Self::UnboundedPreceding => Ok(QueryFrameBound::unbounded_preceding()),
            Self::Preceding(expression) => Ok(QueryFrameBound::preceding(
                expression.resolve_in(context, scope)?,
            )),
            Self::CurrentRow => Ok(QueryFrameBound::current_row()),
            Self::Following(expression) => Ok(QueryFrameBound::following(
                expression.resolve_in(context, scope)?,
            )),
            Self::UnboundedFollowing => Ok(QueryFrameBound::unbounded_following()),
        }
    }
}
impl Lock {
    pub(super) fn resolve_list(
        values: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryLock>, AnalysisError> {
        let mut result = Vec::new();
        for lock in values {
            let mut relations = Vec::new();
            for name in &lock.relations {
                let resolution = scope.resolve_lock(&name.0.join("."));
                relations.push(Order::checked_name(context, &name.0.join("."), resolution)?);
            }
            result.push(QueryLock::new(
                relations,
                lock.strength
                    .map(|strength| strength.semantic_name().to_owned()),
                lock.wait.map(|wait| wait.semantic_name().to_owned()),
            ));
        }
        Ok(result)
    }
}
impl LockStrength {
    const fn semantic_name(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::KeyShare => "KEY SHARE",
            Self::Share => "SHARE",
            Self::NoKeyUpdate => "NO KEY UPDATE",
            Self::Update => "UPDATE",
        }
    }
}
impl LockWait {
    const fn semantic_name(self) -> &'static str {
        match self {
            Self::Block => "BLOCK",
            Self::Skip => "SKIP",
            Self::Error => "ERROR",
        }
    }
}

impl Expr {
    fn sqlite_comparison_sql(&self) -> Option<String> {
        use crate::ast::deparse::{SqlEmit, SqlWriter};
        if let Self::Parenthesized(values) = self
            && let [expression] = values.as_slice()
        {
            return expression.sqlite_comparison_sql();
        }
        let mut writer = SqlWriter::new(Dialect::sqlite());
        self.emit(&mut writer).ok()?;
        Some(writer.into_sql())
    }
}
impl crate::ast::QueryBody {
    fn match_sqlite_order(&self, expression: &Expr) -> Option<usize> {
        match self {
            Self::Select(select) => select.match_sqlite_order(expression),
            Self::Set(set) => {
                if let Some(index) = set.left.body.match_sqlite_order(expression) {
                    Some(index)
                } else {
                    set.right.body.match_sqlite_order(expression)
                }
            }
            Self::Values(rows) => {
                let key = expression.sqlite_comparison_sql()?;
                rows.first()?
                    .iter()
                    .position(|value| value.sqlite_comparison_sql().as_ref() == Some(&key))
            }
        }
    }
}
impl crate::ast::Select {
    fn match_sqlite_order(&self, expression: &Expr) -> Option<usize> {
        let key = expression.sqlite_comparison_sql()?;
        self.targets.iter().position(|target| {
            target.expression.sqlite_comparison_sql().as_ref() == Some(&key)
                || expression
                    .unqualified_column()
                    .is_some_and(|name| target.alias.as_deref() == Some(name))
        })
    }
}
