mod destination;
mod view_support;

pub(crate) use self::destination::UpdateDestination;
pub use self::view_support::{ViewUpdateInfoRead, ViewUpdateSupport};

use crate::ast::TryFromWithTokens;
use crate::sem::traversal::SemanticVisitor;
use crate::sem::{
    AnalysisError, AnalysisProblem, ColumnDefinition, FromClause, ParseContext, QueryCte,
    ResultColumnCollection, SelectStatement, SemAst, SemScalarExpr, TableName, TypeReference,
};

/// An analyzed UPDATE, including its destination, sources and RETURNING outputs.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateStatement {
    target: TableName,
    assignments: Vec<UpdateAssignment>,
    with: Vec<QueryCte>,
    from: FromClause,
    predicate: Option<SemScalarExpr>,
    returning: ResultColumnCollection,
    view_support: Option<ViewUpdateSupport>,
    or_conflict: Option<crate::sem::ConflictClause>,
    inherit: bool,
    indexed: Option<String>,
    not_indexed: bool,
}
impl UpdateStatement {
    pub(crate) fn new(
        target: &TableName,
        assignments: &[UpdateAssignment],
        with: &[QueryCte],
        from: &FromClause,
        predicate: &Option<SemScalarExpr>,
        returning: &ResultColumnCollection,
    ) -> Self {
        Self {
            target: target.clone(),
            assignments: assignments.to_vec(),
            with: with.to_vec(),
            from: from.clone(),
            predicate: predicate.clone(),
            returning: returning.clone(),
            view_support: None,
            or_conflict: None,
            inherit: false,
            indexed: None,
            not_indexed: false,
        }
    }
    pub(crate) fn set_modifiers(
        &mut self,
        support: Option<&ViewUpdateSupport>,
        conflict: &Option<crate::sem::ConflictClause>,
        indexed: &Option<String>,
        not_indexed: bool,
        inherit: bool,
    ) {
        self.view_support = support.cloned();
        self.or_conflict = conflict.clone();
        self.indexed = indexed.clone();
        self.not_indexed = not_indexed;
        self.inherit = inherit;
    }
    /// Qualified catalog destination identity.
    pub const fn target(&self) -> &TableName {
        &self.target
    }
    /// Assignments in SQL order; RHS bindings refer to the pre-update row.
    pub fn assignments(&self) -> &[UpdateAssignment] {
        &self.assignments
    }
    /// Local WITH definitions in declaration order.
    pub fn with(&self) -> &[QueryCte] {
        &self.with
    }
    /// Bound target and FROM sources.
    pub const fn from(&self) -> &FromClause {
        &self.from
    }
    /// Resolved WHERE expression.
    pub const fn predicate(&self) -> Option<&SemScalarExpr> {
        self.predicate.as_ref()
    }
    /// RETURNING outputs; empty when RETURNING is absent.
    pub const fn result_columns(&self) -> &ResultColumnCollection {
        &self.returning
    }
    /// Catalog decision for an ordinary view destination.
    pub const fn view_support(&self) -> Option<&ViewUpdateSupport> {
        self.view_support.as_ref()
    }
    /// Explicit SQLite conflict policy.
    pub const fn or_conflict(&self) -> Option<&crate::sem::ConflictClause> {
        self.or_conflict.as_ref()
    }
    /// Whether PostgreSQL descendants are included (false for ONLY).
    pub const fn includes_descendants(&self) -> bool {
        self.inherit
    }
    /// Explicit SQLite INDEXED BY name.
    pub fn indexed(&self) -> Option<&str> {
        self.indexed.as_deref()
    }
    /// Whether SQLite NOT INDEXED was specified.
    pub const fn not_indexed(&self) -> bool {
        self.not_indexed
    }
    pub(crate) fn rename_columns(&mut self, names: &[String]) {
        self.returning.rename(names);
    }
    pub(crate) fn validate_assignments(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.walk(&mut AssignmentChecker { context })
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.enter_update();
        visitor.enter_query();
        let result = self.walk_body(visitor);
        visitor.leave_query();
        visitor.leave_update();
        result
    }
    fn walk_body<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        for cte in &mut self.with {
            cte.walk(visitor)?;
        }
        self.from.walk(visitor)?;
        crate::sem::traversal::optional(&mut self.predicate, visitor)?;
        if let Some(predicate) = &self.predicate {
            visitor.predicate(predicate);
        }
        // PostgreSQL transforms WHERE and RETURNING before SET assignments.
        // This order matters when RETURNING resolves an unknown parameter.
        visitor.update_returning(&mut self.returning)?;
        for assignment in &mut self.assignments {
            assignment.walk(visitor)?;
        }
        Ok(())
    }
}

/// An UPDATE destination with its original catalog column and resolved value.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateAssignment {
    column: ColumnDefinition,
    destination: SemScalarExpr,
    value: UpdateValue,
}
impl UpdateAssignment {
    pub(crate) fn new(
        column: &ColumnDefinition,
        destination: &SemScalarExpr,
        value: &UpdateValue,
    ) -> Self {
        Self {
            column: column.clone(),
            destination: destination.clone(),
            value: value.clone(),
        }
    }
    /// Original catalog definition, including generated and identity constraints.
    pub const fn column(&self) -> &ColumnDefinition {
        &self.column
    }
    /// Resolved destination column, subfield or subscript.
    pub const fn destination(&self) -> &SemScalarExpr {
        &self.destination
    }
    /// RHS expression, row-subquery output, or SQL DEFAULT.
    pub const fn value(&self) -> &UpdateValue {
        &self.value
    }
    fn walk<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.destination.walk_assignment_target(visitor)?;
        self.value
            .walk(visitor, self.destination.get_type().as_ref())
    }
}

/// A resolved UPDATE RHS without substituting a value for DEFAULT.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum UpdateValue {
    /// SQL DEFAULT for this destination.
    Default,
    /// An ordinary scalar expression.
    Expression(Box<SemScalarExpr>),
    /// A selected output of a row-valued subquery.
    Query(Box<UpdateQueryValue>),
}
/// A row-subquery output selected by zero-based position.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateQueryValue {
    query: SelectStatement,
    index: usize,
}
impl UpdateQueryValue {
    /// Full resolved subquery; analysis does not execute it.
    pub const fn query(&self) -> &SelectStatement {
        &self.query
    }
    /// Zero-based output position assigned to the destination.
    pub const fn index(&self) -> usize {
        self.index
    }
}
impl UpdateValue {
    pub(crate) const fn default_value() -> Self {
        Self::Default
    }
    pub(crate) fn expression(value: &SemScalarExpr) -> Self {
        Self::Expression(Box::new(value.clone()))
    }
    pub(crate) fn query(query: &SelectStatement, index: usize) -> Self {
        Self::Query(Box::new(UpdateQueryValue {
            query: query.clone(),
            index,
        }))
    }
    /// Whether SQL explicitly requested DEFAULT.
    pub const fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }
    /// Ordinary scalar RHS, absent for DEFAULT and row-subquery outputs.
    pub const fn as_expression(&self) -> Option<&SemScalarExpr> {
        match self {
            Self::Expression(value) => Some(value),
            _ => None,
        }
    }
    /// Row-subquery output, when this RHS uses that form.
    pub const fn as_query(&self) -> Option<&UpdateQueryValue> {
        match self {
            Self::Query(value) => Some(value),
            _ => None,
        }
    }
    fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        ty: Option<&TypeReference>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Default => {}
            Self::Expression(value) => {
                value.walk(visitor)?;
                if let Some(ty) = ty {
                    visitor.assignment_value(value, ty)?;
                }
            }
            Self::Query(value) => {
                value.query.walk(visitor)?;
                if let Some(ty) = ty {
                    value.query.assign_update_output(visitor, value.index, ty)?;
                }
            }
        }
        Ok(())
    }
}
struct AssignmentChecker<'a, C> {
    context: &'a mut C,
}
impl<C: ParseContext> SemanticVisitor for AssignmentChecker<'_, C> {
    fn assignment_value(
        &mut self,
        value: &mut SemScalarExpr,
        ty: &TypeReference,
    ) -> Result<(), AnalysisError> {
        if self.context.dialect() == crate::ast::Dialect::sqlite() {
            return Ok(());
        }
        let unknown = value
            .as_const()
            .is_some_and(|c| c.is_null() || c.as_string().is_some());
        if let Some(source) = value.get_type()
            && !unknown
            && &source != ty
            && self.context.get_assignment_cast(&source, ty).is_none()
        {
            self.context
                .report_analysis_problem(AnalysisProblem::update(
                    crate::sem::problem::UpdateProblemKind::assignment_type(),
                    &format!("cannot assign {source:?} to {ty:?}"),
                ))?;
        }
        if (value.get_type().is_some() || unknown)
            && value.get_type().as_ref() != Some(ty)
            && !value.needs_type_inference()
        {
            *value = SemScalarExpr::wrap_implicit_cast(ty, value);
        }
        Ok(())
    }
}
/// Analyze PostgreSQL UPDATE through the common AST and ParseContext.
///
/// # Errors
/// Returns an error when syntax conversion or semantic analysis fails.
pub fn analyze_update_statement<T: ParseContext>(
    mut context: T,
    _parent_schema: &Option<String>,
    syn: crate::syn::UpdateStmt,
) -> Result<(SemAst, T), AnalysisError> {
    AnalysisError::check_dialect(crate::ast::Dialect::postgres(), context.dialect())?;
    let syntax = crate::ast::Update::try_from_with_tokens(&syn, &::sql_fun_core::IVec::default())?;
    let update = syntax.resolve(&mut context)?;
    Ok((SemAst::update(&update), context))
}
