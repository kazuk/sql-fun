//! Clause structure retained after name resolution.
use crate::sem::traversal::SemanticVisitor;
use crate::sem::{
    AliasName, AnalysisError, OperatorName, ResultColumnCollection, SelectStatement, SemScalarExpr,
    TypeReference,
};

/// The completed outcome of resolving a clause name.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum QueryNameResolution {
    /// Reference to a window definition in the current query.
    WindowAlias(usize),
    /// Reference to a FROM source.
    SourceAlias(AliasName),
    /// An operator name found in the catalog (overload selection is separate).
    Operator(OperatorName),
    /// Reference to a function definition, independently of overload selection.
    Function(crate::sem::FullName),
    /// Reference to a catalog type.
    Type(crate::sem::TypeReference),
    /// SQLite cast names denote affinity, independently of a type catalog.
    Affinity(String),
    /// Lookup found no definition.
    NotFound,
    /// Lookup found more than one applicable definition.
    Ambiguous,
}
impl QueryNameResolution {
    pub(crate) const fn function_identity(&self) -> Option<&crate::sem::FullName> {
        if let Self::Function(name) = self {
            Some(name)
        } else {
            None
        }
    }

    pub(crate) fn type_reference(value: &crate::sem::TypeReference) -> Self {
        Self::Type(value.clone())
    }
    pub(crate) fn affinity(value: &str) -> Self {
        Self::Affinity(value.to_owned())
    }

    pub(crate) fn function(name: &crate::sem::FullName) -> Self {
        Self::Function(name.clone())
    }
    pub(crate) const fn window(index: usize) -> Self {
        Self::WindowAlias(index)
    }
    pub(crate) fn source(alias: &AliasName) -> Self {
        Self::SourceAlias(alias.clone())
    }
    pub(crate) fn operator(operator: &OperatorName) -> Self {
        Self::Operator(operator.clone())
    }
    pub(crate) const fn not_found() -> Self {
        Self::NotFound
    }
    pub(crate) const fn ambiguous() -> Self {
        Self::Ambiguous
    }
    pub(crate) const fn window_index(&self) -> Option<usize> {
        match self {
            Self::WindowAlias(index) => Some(*index),
            _ => None,
        }
    }
    pub(crate) fn reference_problem(&self, name: &str) -> Option<crate::sem::QueryProblem> {
        match self {
            Self::NotFound => Some(crate::sem::QueryProblem::reference_not_found(name)),
            Self::Ambiguous => Some(crate::sem::QueryProblem::ambiguous_reference(name)),
            _ => None,
        }
    }
    /// Whether name lookup succeeded; operator/function overload selection is separate.
    pub const fn is_resolved(&self) -> bool {
        !matches!(self, Self::NotFound | Self::Ambiguous)
    }
}

/// A clause name together with its completed binding outcome.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryName {
    name: String,
    resolution: QueryNameResolution,
}
impl QueryName {
    pub(crate) fn new(name: &str, resolution: QueryNameResolution) -> Self {
        Self {
            name: name.to_owned(),
            resolution,
        }
    }
    /// Original SQL name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Recorded binding; no further lookup is required to inspect it.
    pub const fn resolution(&self) -> &QueryNameResolution {
        &self.resolution
    }
}

/// Ordering expression and its SQL ordering modifiers.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryOrder {
    expression: SemScalarExpr,
    descending: Option<bool>,
    nulls_first: Option<bool>,
    using: Option<ResolvedOperator>,
}
impl QueryOrder {
    pub(crate) const fn new(
        expression: SemScalarExpr,
        descending: Option<bool>,
        nulls_first: Option<bool>,
        using: Option<ResolvedOperator>,
    ) -> Self {
        Self {
            expression,
            descending,
            nulls_first,
            using,
        }
    }
    /// Resolved ordering expression.
    pub const fn expression(&self) -> &SemScalarExpr {
        &self.expression
    }
    /// Explicit direction, if supplied.
    pub const fn descending(&self) -> Option<bool> {
        self.descending
    }
    /// Explicit null ordering, if supplied.
    pub const fn nulls_first(&self) -> Option<bool> {
        self.nulls_first
    }
    /// Resolved USING operator name, if supplied.
    pub const fn using(&self) -> Option<&ResolvedOperator> {
        self.using.as_ref()
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.expression.walk(visitor)?;
        if let Some(operator) = &self.using {
            visitor.order_operator(&self.expression, operator)?;
        }
        Ok(())
    }
}

/// Recursive grouping structure, including empty grouping sets.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum QueryGrouping {
    /// Ordinary grouping expression.
    Expression(Box<SemScalarExpr>),
    /// Empty grouping set.
    Empty,
    /// ROLLUP.
    Rollup(Vec<Self>),
    /// CUBE.
    Cube(Vec<Self>),
    /// GROUPING SETS.
    Sets(Vec<Self>),
    /// Parenthesized group.
    Simple(Vec<Self>),
}
impl QueryGrouping {
    pub(crate) fn expression(expression: SemScalarExpr) -> Self {
        Self::Expression(Box::new(expression))
    }
    pub(crate) const fn empty() -> Self {
        Self::Empty
    }
    pub(crate) const fn rollup(groups: Vec<Self>) -> Self {
        Self::Rollup(groups)
    }
    pub(crate) const fn cube(groups: Vec<Self>) -> Self {
        Self::Cube(groups)
    }
    pub(crate) const fn sets(groups: Vec<Self>) -> Self {
        Self::Sets(groups)
    }
    pub(crate) const fn simple(groups: Vec<Self>) -> Self {
        Self::Simple(groups)
    }
    fn walk<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(expression) => expression.walk(visitor),
            Self::Empty => Ok(()),
            Self::Rollup(groups)
            | Self::Cube(groups)
            | Self::Sets(groups)
            | Self::Simple(groups) => {
                for group in groups {
                    group.walk(visitor)?;
                }
                Ok(())
            }
        }
    }
}

/// A window-frame boundary.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum QueryFrameBound {
    /// UNBOUNDED PRECEDING.
    UnboundedPreceding,
    /// An offset PRECEDING.
    Preceding(SemScalarExpr),
    /// CURRENT ROW.
    CurrentRow,
    /// An offset FOLLOWING.
    Following(SemScalarExpr),
    /// UNBOUNDED FOLLOWING.
    UnboundedFollowing,
}
impl QueryFrameBound {
    pub(crate) const fn unbounded_preceding() -> Self {
        Self::UnboundedPreceding
    }
    pub(crate) const fn preceding(value: SemScalarExpr) -> Self {
        Self::Preceding(value)
    }
    pub(crate) const fn current_row() -> Self {
        Self::CurrentRow
    }
    pub(crate) const fn following(value: SemScalarExpr) -> Self {
        Self::Following(value)
    }
    pub(crate) const fn unbounded_following() -> Self {
        Self::UnboundedFollowing
    }
    fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        mode: &str,
        order: &[QueryOrder],
        reference: Option<&QueryName>,
    ) -> Result<(), AnalysisError> {
        if let Self::Preceding(value) | Self::Following(value) = self {
            value.walk(visitor)?;
            visitor.frame_offset(value, mode, order, reference);
        }
        Ok(())
    }
}

/// A retained window frame. Frame legality checking is independent of name binding.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryFrame {
    mode: String,
    start: QueryFrameBound,
    end: Option<QueryFrameBound>,
    exclusion: Option<String>,
}
impl QueryFrame {
    pub(crate) fn new(
        mode: &str,
        start: QueryFrameBound,
        end: Option<QueryFrameBound>,
        exclusion: Option<String>,
    ) -> Self {
        Self {
            mode: mode.to_owned(),
            start,
            end,
            exclusion,
        }
    }
    /// Frame mode: ROWS, RANGE, or GROUPS.
    pub fn mode(&self) -> &str {
        &self.mode
    }
    /// Starting bound.
    pub const fn start(&self) -> &QueryFrameBound {
        &self.start
    }
    /// Ending bound, when explicitly supplied.
    pub const fn end(&self) -> Option<&QueryFrameBound> {
        self.end.as_ref()
    }
    /// Explicit exclusion.
    pub fn exclusion(&self) -> Option<&str> {
        self.exclusion.as_deref()
    }
}

/// Named or inline window specification, with explicit reference resolution.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryWindow {
    name: Option<String>,
    reference: Option<QueryName>,
    reference_only: bool,
    partition: Vec<SemScalarExpr>,
    order: Vec<QueryOrder>,
    frame: Option<QueryFrame>,
}
impl QueryWindow {
    pub(crate) fn ordering_type(
        &self,
        inherited: &[Option<crate::sem::TypeReference>],
    ) -> Option<crate::sem::TypeReference> {
        match self.order.as_slice() {
            [order] => order.expression.get_type(),
            [] => {
                let index = self.reference.as_ref()?.resolution.window_index()?;
                inherited.get(index)?.clone()
            }
            _ => None,
        }
    }

    pub(crate) const fn new(
        name: Option<String>,
        reference: Option<QueryName>,
        reference_only: bool,
        partition: Vec<SemScalarExpr>,
        order: Vec<QueryOrder>,
        frame: Option<QueryFrame>,
    ) -> Self {
        Self {
            name,
            reference,
            reference_only,
            partition,
            order,
            frame,
        }
    }
    /// Window definition name, if this declares one.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// Completed resolution of a referenced window.
    pub const fn reference(&self) -> Option<&QueryName> {
        self.reference.as_ref()
    }
    /// Whether this is OVER name rather than an inline specification.
    pub const fn reference_only(&self) -> bool {
        self.reference_only
    }
    /// Partition expressions.
    pub fn partition(&self) -> &[SemScalarExpr] {
        &self.partition
    }
    /// Local ordering expressions.
    pub fn order(&self) -> &[QueryOrder] {
        &self.order
    }
    /// Local frame specification.
    pub const fn frame(&self) -> Option<&QueryFrame> {
        self.frame.as_ref()
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for expression in &mut self.partition {
            expression.walk(visitor)?;
        }
        for order in &mut self.order {
            order.walk(visitor)?;
        }
        if let Some(frame) = &mut self.frame {
            frame
                .start
                .walk(visitor, &frame.mode, &self.order, self.reference.as_ref())?;
            if let Some(end) = &mut frame.end {
                end.walk(visitor, &frame.mode, &self.order, self.reference.as_ref())?;
            }
        }
        Ok(())
    }
}

/// SELECT-specific clauses, separate from query-wide trailing clauses.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectClauses {
    distinct: Option<Vec<SemScalarExpr>>,
    group: Vec<QueryGrouping>,
    group_distinct: bool,
    having: Option<SemScalarExpr>,
    windows: Vec<QueryWindow>,
    into: Option<SelectDestination>,
}
impl SelectClauses {
    pub(crate) fn set_destination(&mut self, destination: Option<SelectDestination>) {
        self.into = destination;
    }
    /// SELECT INTO destination declaration; this defines a name rather than referencing a source.
    pub const fn destination(&self) -> Option<&SelectDestination> {
        self.into.as_ref()
    }

    pub(crate) const fn new(
        distinct: Option<Vec<SemScalarExpr>>,
        group: Vec<QueryGrouping>,
        group_distinct: bool,
        having: Option<SemScalarExpr>,
        windows: Vec<QueryWindow>,
    ) -> Self {
        Self {
            distinct,
            group,
            group_distinct,
            having,
            windows,
            into: None,
        }
    }
    /// DISTINCT ON expressions, empty for plain DISTINCT, absent for ALL.
    pub const fn distinct(&self) -> Option<&Vec<SemScalarExpr>> {
        self.distinct.as_ref()
    }
    /// Structured grouping.
    pub fn group(&self) -> &[QueryGrouping] {
        &self.group
    }
    /// Whether GROUP BY DISTINCT was specified.
    pub const fn group_distinct(&self) -> bool {
        self.group_distinct
    }
    /// Resolved HAVING predicate.
    pub const fn having(&self) -> Option<&SemScalarExpr> {
        self.having.as_ref()
    }
    /// Named window definitions.
    pub fn windows(&self) -> &[QueryWindow] {
        &self.windows
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        if let Some(distinct) = &mut self.distinct {
            for expression in distinct {
                expression.walk(visitor)?;
            }
        }
        for group in &mut self.group {
            group.walk(visitor)?;
        }
        if let Some(having) = &mut self.having {
            having.walk(visitor)?;
            visitor.predicate(having);
        }
        Ok(())
    }
    pub(crate) fn walk_windows<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for window in &mut self.windows {
            window.walk(visitor)?;
            visitor.window_definition(window);
        }
        Ok(())
    }
}

/// Function-specific SQL modifiers, retained independently of overload resolution.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionModifiers {
    distinct: bool,
    star: bool,
    variadic: bool,
    within_group: bool,
    within_order: Vec<QueryOrder>,
    order: Vec<QueryOrder>,
    filter: Option<SemScalarExpr>,
    over: Option<QueryWindow>,
}
impl FunctionModifiers {
    pub(crate) fn new(
        within_order: Vec<QueryOrder>,
        order: Vec<QueryOrder>,
        filter: Option<SemScalarExpr>,
        over: Option<QueryWindow>,
    ) -> Self {
        Self {
            within_order,
            order,
            filter,
            over,
            ..Self::default()
        }
    }
    pub(crate) const fn set_flags(
        &mut self,
        distinct: bool,
        star: bool,
        variadic: bool,
        within_group: bool,
    ) {
        self.distinct = distinct;
        self.star = star;
        self.variadic = variadic;
        self.within_group = within_group;
    }
    /// DISTINCT arguments.
    pub const fn distinct(&self) -> bool {
        self.distinct
    }
    /// Star argument, such as count(*).
    pub const fn star(&self) -> bool {
        self.star
    }
    /// Explicit VARIADIC argument.
    pub const fn variadic(&self) -> bool {
        self.variadic
    }
    /// WITHIN GROUP was specified.
    pub const fn within_group(&self) -> bool {
        self.within_group
    }
    /// Aggregate FILTER predicate.
    pub const fn filter(&self) -> Option<&SemScalarExpr> {
        self.filter.as_ref()
    }
    /// Resolved OVER specification.
    pub const fn over(&self) -> Option<&QueryWindow> {
        self.over.as_ref()
    }
    /// Aggregate argument ordering.
    pub fn order(&self) -> &[QueryOrder] {
        &self.order
    }
    /// WITHIN GROUP ordering.
    pub fn within_order(&self) -> &[QueryOrder] {
        &self.within_order
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for order in &mut self.order {
            order.walk(visitor)?;
        }
        for order in &mut self.within_order {
            order.walk(visitor)?;
        }
        if let Some(filter) = &mut self.filter {
            filter.walk(visitor)?;
            visitor.predicate(filter);
        }
        if let Some(over) = &mut self.over {
            over.walk(visitor)?;
        }
        Ok(())
    }
}

/// A locking clause with resolved source references.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryLock {
    relations: Vec<QueryName>,
    strength: Option<String>,
    wait: Option<String>,
}
impl QueryLock {
    pub(crate) const fn new(
        relations: Vec<QueryName>,
        strength: Option<String>,
        wait: Option<String>,
    ) -> Self {
        Self {
            relations,
            strength,
            wait,
        }
    }
    /// Explicitly named lock targets and their binding outcomes.
    pub fn relations(&self) -> &[QueryName] {
        &self.relations
    }
    /// Requested lock strength.
    pub fn strength(&self) -> Option<&str> {
        self.strength.as_deref()
    }
    /// Requested wait policy.
    pub fn wait(&self) -> Option<&str> {
        self.wait.as_deref()
    }
}

/// A query boundary owning local WITH definitions and trailing clauses.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SemanticQuery {
    with: Vec<QueryCte>,
    recursive: bool,
    body: SelectStatement,
    order: Vec<QueryOrder>,
    limit: Option<SemScalarExpr>,
    offset: Option<SemScalarExpr>,
    with_ties: bool,
    set_all: bool,
    locks: Vec<QueryLock>,
}
impl SemanticQuery {
    pub(crate) const fn new(
        with: Vec<QueryCte>,
        recursive: bool,
        body: SelectStatement,
        order: Vec<QueryOrder>,
        limit: Option<SemScalarExpr>,
        offset: Option<SemScalarExpr>,
    ) -> Self {
        Self {
            with,
            recursive,
            body,
            order,
            limit,
            offset,
            with_ties: false,
            set_all: false,
            locks: Vec::new(),
        }
    }
    pub(crate) fn set_modifiers(&mut self, with_ties: bool, set_all: bool, locks: Vec<QueryLock>) {
        self.with_ties = with_ties;
        self.set_all = set_all;
        self.locks = locks;
    }
    /// Local WITH entries, in declaration order.
    pub fn with(&self) -> &[QueryCte] {
        &self.with
    }
    /// Whether WITH RECURSIVE was specified (recursive binding remains separate).
    pub const fn recursive(&self) -> bool {
        self.recursive
    }
    /// Query body; trailing clauses are owned by this boundary.
    pub const fn body(&self) -> &SelectStatement {
        &self.body
    }
    pub(crate) const fn body_mut(&mut self) -> &mut SelectStatement {
        &mut self.body
    }
    /// Query ordering.
    pub fn order(&self) -> &[QueryOrder] {
        &self.order
    }
    /// LIMIT or FETCH expression.
    pub const fn limit(&self) -> Option<&SemScalarExpr> {
        self.limit.as_ref()
    }
    /// OFFSET expression.
    pub const fn offset(&self) -> Option<&SemScalarExpr> {
        self.offset.as_ref()
    }
    /// FETCH WITH TIES.
    pub const fn with_ties(&self) -> bool {
        self.with_ties
    }
    /// ALL modifier on this boundary's set operation.
    pub const fn set_all(&self) -> bool {
        self.set_all
    }
    /// Locking clauses.
    pub fn locks(&self) -> &[QueryLock] {
        &self.locks
    }
    pub(crate) fn walk_assigned<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        targets: &[crate::sem::InsertTarget],
    ) -> Result<(), AnalysisError> {
        for entry in &mut self.with {
            entry.walk(visitor)?;
        }
        self.body.walk_body_assigned(visitor, targets)?;
        for order in &mut self.order {
            order.walk(visitor)?;
        }
        if let Some(limit) = &mut self.limit {
            limit.walk(visitor)?;
            visitor.row_count(limit);
        }
        if let Some(offset) = &mut self.offset {
            offset.walk(visitor)?;
            visitor.row_count(offset);
        }
        Ok(())
    }
}

/// VALUES retains every row independently of result-type unification.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SemanticValues {
    rows: Vec<Vec<SemScalarExpr>>,
    columns: ResultColumnCollection,
}
impl SemanticValues {
    pub(crate) fn assign_update_output<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        index: usize,
        ty: &crate::sem::TypeReference,
    ) -> Result<(), AnalysisError> {
        for row in &mut self.rows {
            if let Some(value) = row.get_mut(index) {
                visitor.assignment_value(value, ty)?;
            }
        }
        self.columns.assign_update_output(visitor, index, ty)
    }

    pub(crate) fn new(rows: Vec<Vec<SemScalarExpr>>) -> Self {
        let mut columns = ResultColumnCollection::default();
        if let Some(first) = rows.first() {
            for (index, value) in first.iter().enumerate() {
                columns.push_column(
                    &crate::sem::ColumnName::from(format!("column{}", index + 1).as_str()),
                    value,
                );
            }
        }
        Self { rows, columns }
    }
    /// Resolved rows, including parameters outside the first row.
    pub fn rows(&self) -> &[Vec<SemScalarExpr>] {
        &self.rows
    }
    pub(crate) const fn columns(&self) -> &ResultColumnCollection {
        &self.columns
    }
    pub(crate) fn rename(&mut self, names: &[String]) {
        self.columns.rename(names);
    }
    pub(crate) fn walk_assigned<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        targets: &[crate::sem::InsertTarget],
    ) -> Result<(), AnalysisError> {
        for (row_index, row) in self.rows.iter_mut().enumerate() {
            for (index, value) in row.iter_mut().enumerate() {
                value.walk(visitor)?;
                if let Some(target) = targets.get(index) {
                    visitor.assignment(value, target)?;
                }
                if row_index == 0 {
                    let column = self
                        .columns
                        .get_column_at(index)
                        .expect("VALUES output position");
                    let name = column
                        .column_name()
                        .as_ref()
                        .map_or("", crate::sem::ColumnName::as_str);
                    visitor.output(name, value);
                    self.columns.refresh_value(index, value);
                }
            }
        }
        Ok(())
    }
}

/// A local CTE definition, preserving declaration order and materialization choice.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryCte {
    name: String,
    columns: Vec<String>,
    materialized: Option<bool>,
    statement: crate::sem::CteStatement,
}
impl QueryCte {
    pub(crate) fn new(
        name: &str,
        columns: Vec<String>,
        materialized: Option<bool>,
        statement: crate::sem::CteStatement,
    ) -> Self {
        Self {
            name: name.to_owned(),
            columns,
            materialized,
            statement,
        }
    }
    /// Declared name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Explicit column aliases.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }
    /// Explicit materialization choice.
    pub const fn materialized(&self) -> Option<bool> {
        self.materialized
    }
    /// Resolved statement.
    pub const fn statement(&self) -> &crate::sem::CteStatement {
        &self.statement
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.statement.walk(visitor)?;
        visitor.cte(&self.name);
        Ok(())
    }
}

/// An operator-name lookup and the candidates it identified before type inference.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedOperator {
    name: OperatorName,
    resolution: QueryNameResolution,
    signatures: Vec<crate::sem::parameter_resolution::Signature>,
}
impl ResolvedOperator {
    pub(crate) fn canonicalize_grouping_key(&mut self) {
        self.signatures.clear();
    }

    pub(crate) fn resolve(
        context: &impl crate::sem::ParseContext,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Self {
        if let Some(operator) = context.get_operator_op(name, left, right) {
            Self {
                name: name.clone(),
                resolution: QueryNameResolution::operator(name),
                // Parameter-free schema expressions need the lookup outcome, not
                // a copy of every overload's inference signature.
                signatures: if context.parameter_inference_needed() {
                    operator.parameter_signatures()
                } else {
                    Vec::new()
                },
            }
        } else {
            Self {
                name: name.clone(),
                resolution: QueryNameResolution::not_found(),
                signatures: Vec::new(),
            }
        }
    }
    pub(crate) fn comparison(context: &impl crate::sem::ParseContext, name: &OperatorName) -> Self {
        let mut binding = Self::resolve(context, name, true, true);
        if let Some(operator) = context.get_operator_op(name, true, true) {
            binding.signatures = operator.parameter_signatures();
        }
        binding
    }

    pub(crate) fn known_comparison(
        &self,
        context: &mut impl crate::sem::ParseContext,
        left: &TypeReference,
        right: &TypeReference,
    ) -> Result<Option<[TypeReference; 2]>, AnalysisError> {
        let definitions =
            crate::sem::parameter_resolution::Signature::binary_definitions(&self.signatures);
        let unknown = crate::sem::MinMaxExpr::unknown();
        let (left, right) = if left == &unknown && right == &unknown {
            (
                crate::sem::PgBuiltInType::text(),
                crate::sem::PgBuiltInType::text(),
            )
        } else if left == &unknown {
            let ty = crate::sem::MinMaxExpr::base_type(context, right);
            (ty.clone(), ty)
        } else if right == &unknown {
            let ty = crate::sem::MinMaxExpr::base_type(context, left);
            (ty.clone(), ty)
        } else {
            (left.clone(), right.clone())
        };
        crate::sem::ArithExpr::known_comparison(context, &definitions, &left, &right)
    }

    pub(crate) fn comparison_signatures(&self) -> &[crate::sem::parameter_resolution::Signature] {
        &self.signatures
    }

    /// SQL operator whose completed lookup this binding retains.
    pub const fn name(&self) -> &OperatorName {
        &self.name
    }

    /// Name lookup outcome; overload/type selection is a subsequent operation.
    pub const fn resolution(&self) -> &QueryNameResolution {
        &self.resolution
    }
    pub(crate) fn infer(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl crate::sem::ParseContext,
        left: Option<&SemScalarExpr>,
        right: Option<&SemScalarExpr>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if left.is_some_and(|value| value.constraint_node().is_none())
            && right.is_some_and(|value| value.constraint_node().is_none())
        {
            return Ok(None);
        }
        if context.dialect() == crate::ast::Dialect::sqlite() {
            return Ok(Some(parameters.sqlite_operator(&self.name, left, right)));
        }
        let arguments = Self::arguments(left, right);
        let result = if self.resolution.is_resolved() {
            parameters.call(
                context,
                &format!("{:?}", self.name),
                &arguments,
                &self.signatures,
            )?
        } else {
            parameters.operation(
                &format!("{:?}", self.name),
                &arguments,
                &crate::sem::InputTypeConstraint::any(),
                None,
            )
        };
        Ok(Some(result))
    }
}

impl ResolvedOperator {
    fn arguments(
        left: Option<&SemScalarExpr>,
        right: Option<&SemScalarExpr>,
    ) -> Vec<SemScalarExpr> {
        left.into_iter().chain(right).cloned().collect()
    }
}

/// A table declared by SELECT INTO, not a reference to an existing table.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectDestination {
    name: crate::sem::FullName,
    persistence: String,
}
impl SelectDestination {
    pub(crate) fn new(name: &crate::sem::FullName, persistence: &str) -> Self {
        Self {
            name: name.clone(),
            persistence: persistence.to_owned(),
        }
    }
    /// Declared destination name.
    pub const fn name(&self) -> &crate::sem::FullName {
        &self.name
    }
    /// PostgreSQL persistence marker supplied by the parser.
    pub fn persistence(&self) -> &str {
        &self.persistence
    }
}
