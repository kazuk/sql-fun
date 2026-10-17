mod arbiter;
pub use arbiter::{
    InsertArbiter, InsertArbiterLookup, InsertConstraintKind, InsertConstraintLookup,
};

use crate::ast::TryFromWithTokens;
use crate::sem::traversal::SemanticVisitor;
use crate::sem::{
    AnalysisError, AnalysisProblem, ColumnDefinition, ParseContext, QueryCte,
    ResultColumnCollection, SelectStatement, SemAst, SemScalarExpr, TableName, TypeReference,
};

/// An analyzed INSERT with destination bindings, source and RETURNING metadata.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertStatement {
    target: TableName,
    columns: Vec<InsertTarget>,
    source: InsertSource,
    with: Vec<QueryCte>,
    returning: ResultColumnCollection,
    conflicts: Vec<InsertConflict>,
    overriding: Option<String>,
    or_conflict: Option<crate::sem::ConflictClause>,
}
impl InsertStatement {
    pub(crate) fn new(
        target: &TableName,
        columns: Vec<InsertTarget>,
        source: InsertSource,
        with: Vec<QueryCte>,
        returning: ResultColumnCollection,
    ) -> Self {
        Self {
            target: target.clone(),
            columns,
            source,
            with,
            returning,
            conflicts: Vec::new(),
            overriding: None,
            or_conflict: None,
        }
    }
    pub(crate) fn set_modifiers(
        &mut self,
        conflicts: Vec<InsertConflict>,
        overriding: Option<String>,
        or_conflict: Option<crate::sem::ConflictClause>,
    ) {
        self.conflicts = conflicts;
        self.overriding = overriding;
        self.or_conflict = or_conflict;
    }
    /// Catalog identity of the writable destination.
    pub const fn target(&self) -> &TableName {
        &self.target
    }
    /// Ordered destination bindings for source values.
    pub fn columns(&self) -> &[InsertTarget] {
        &self.columns
    }
    /// Resolved source, distinguishing DEFAULT VALUES and per-value DEFAULT.
    pub const fn source(&self) -> &InsertSource {
        &self.source
    }
    /// Local WITH definitions in declaration order.
    pub fn with(&self) -> &[QueryCte] {
        &self.with
    }
    /// RETURNING metadata; empty when RETURNING is absent.
    pub const fn result_columns(&self) -> &ResultColumnCollection {
        &self.returning
    }
    /// Resolved DO NOTHING clauses in their original order.
    pub fn conflicts(&self) -> &[InsertConflict] {
        &self.conflicts
    }
    /// PostgreSQL identity override mode, when specified.
    pub fn overriding(&self) -> Option<&str> {
        self.overriding.as_deref()
    }
    /// SQLite statement-level conflict policy, when specified.
    pub const fn or_conflict(&self) -> Option<&crate::sem::ConflictClause> {
        self.or_conflict.as_ref()
    }
    pub(crate) fn rename_columns(&mut self, columns: &[String]) {
        self.returning.rename(columns);
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
        visitor.enter_query();
        let result = self.walk_body(visitor);
        visitor.leave_query();
        result
    }
    fn walk_body<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        for entry in &mut self.with {
            entry.walk(visitor)?;
        }
        self.source.walk(visitor, &mut self.columns)?;
        for conflict in &mut self.conflicts {
            conflict.walk(visitor)?;
        }
        self.returning.walk(visitor)
    }
}

/// One writable column or subfield bound to a source position.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertTarget {
    column: ColumnDefinition,
    destination: SemScalarExpr,
}
impl InsertTarget {
    pub(crate) fn new(column: &ColumnDefinition, destination: SemScalarExpr) -> Self {
        Self {
            column: column.clone(),
            destination,
        }
    }
    /// Original catalog column, retaining default/generated/identity constraints.
    pub const fn column(&self) -> &ColumnDefinition {
        &self.column
    }
    /// Bound column or subfield, including resolved subscripts.
    pub const fn destination(&self) -> &SemScalarExpr {
        &self.destination
    }
    pub(crate) fn assignment_type(&self) -> Option<TypeReference> {
        self.destination.get_type()
    }
}

/// INSERT row source.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InsertSource {
    /// Every destination column receives its SQL default.
    DefaultValues,
    /// Direct VALUES with retained per-value DEFAULT markers.
    Values(Vec<Vec<InsertValue>>),
    /// A SELECT, compound query or decorated VALUES source.
    Query(Box<SelectStatement>),
}
impl InsertSource {
    pub(crate) const fn default_values() -> Self {
        Self::DefaultValues
    }
    pub(crate) const fn values(rows: Vec<Vec<InsertValue>>) -> Self {
        Self::Values(rows)
    }
    pub(crate) fn query(query: SelectStatement) -> Self {
        Self::Query(Box::new(query))
    }
    /// Direct VALUES rows, if this source uses that form.
    pub fn rows(&self) -> Option<&[Vec<InsertValue>]> {
        match self {
            Self::Values(rows) => Some(rows),
            _ => None,
        }
    }
    /// Resolved query, if this source uses that form.
    pub const fn as_query(&self) -> Option<&SelectStatement> {
        match self {
            Self::Query(query) => Some(query),
            _ => None,
        }
    }
    pub(crate) fn width(&self) -> usize {
        match self {
            Self::DefaultValues => 0,
            Self::Values(rows) => rows.first().map_or(0, Vec::len),
            Self::Query(query) => query.result_columns().len(),
        }
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        targets: &mut [InsertTarget],
    ) -> Result<(), AnalysisError> {
        for target in targets.iter_mut() {
            target.destination.walk_assignment_target(visitor)?;
        }
        match self {
            Self::DefaultValues => Ok(()),
            Self::Values(rows) => {
                for row in rows {
                    for (index, value) in row.iter_mut().enumerate() {
                        if let Some(target) = targets.get(index) {
                            value.walk(visitor, target)?;
                        } else if let Some(value) = value.expression_mut() {
                            value.walk(visitor)?;
                        }
                    }
                }
                Ok(())
            }
            Self::Query(query) => query.walk_assigned(visitor, targets),
        }
    }
}

/// A direct INSERT value; DEFAULT is not a fabricated scalar expression.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InsertValue {
    /// SQL DEFAULT for the corresponding destination.
    Default,
    /// A name-resolved input expression.
    Expression(Box<SemScalarExpr>),
}
impl InsertValue {
    pub(crate) const fn default_value() -> Self {
        Self::Default
    }
    pub(crate) fn expression(value: SemScalarExpr) -> Self {
        Self::Expression(Box::new(value))
    }
    /// Whether this value requests the SQL default.
    pub const fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }
    /// Input expression, absent for DEFAULT.
    pub const fn as_expression(&self) -> Option<&SemScalarExpr> {
        match self {
            Self::Expression(value) => Some(value),
            Self::Default => None,
        }
    }
    fn expression_mut(&mut self) -> Option<&mut SemScalarExpr> {
        match self {
            Self::Expression(value) => Some(value),
            Self::Default => None,
        }
    }
    fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        target: &InsertTarget,
    ) -> Result<(), AnalysisError> {
        if let Self::Expression(value) = self {
            value.walk(visitor)?;
            visitor.assignment(value, target)?;
        }
        Ok(())
    }
}

/// A bound ON CONFLICT DO NOTHING clause.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertConflict {
    inference: Vec<SemScalarExpr>,
    predicate: Option<SemScalarExpr>,
    arbiter: InsertArbiterLookup,
}
impl InsertConflict {
    pub(crate) const fn new(
        inference: Vec<SemScalarExpr>,
        predicate: Option<SemScalarExpr>,
        constraint: Option<&str>,
    ) -> Self {
        let arbiter = InsertArbiterLookup::initial(!inference.is_empty() || constraint.is_some());
        Self {
            inference,
            predicate,
            arbiter,
        }
    }
    /// Resolved conflict-target expressions.
    pub fn inference(&self) -> &[SemScalarExpr] {
        &self.inference
    }
    /// Resolved partial-index predicate.
    pub const fn predicate(&self) -> Option<&SemScalarExpr> {
        self.predicate.as_ref()
    }
    /// Completed lookup of conflict arbiters, including failed lookup outcomes.
    pub const fn arbiter(&self) -> &InsertArbiterLookup {
        &self.arbiter
    }
    pub(crate) fn set_arbiter(&mut self, arbiter: InsertArbiterLookup) {
        self.arbiter = arbiter;
    }
    fn walk<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        for expression in &mut self.inference {
            expression.walk(visitor)?;
        }
        crate::sem::traversal::optional(&mut self.predicate, visitor)?;
        if let Some(predicate) = &self.predicate {
            visitor.predicate(predicate);
        }
        Ok(())
    }
}

struct AssignmentChecker<'a, C> {
    context: &'a mut C,
}
impl<C: ParseContext> SemanticVisitor for AssignmentChecker<'_, C> {
    fn assignment(
        &mut self,
        value: &mut SemScalarExpr,
        target: &InsertTarget,
    ) -> Result<(), AnalysisError> {
        if self.context.dialect() == crate::ast::Dialect::sqlite() {
            return Ok(());
        }
        let Some(ty) = target.assignment_type() else {
            return Ok(());
        };
        let unknown_literal = value
            .as_const()
            .is_some_and(|constant| constant.is_null() || constant.as_string().is_some());
        if let Some(source) = value.get_type()
            && !unknown_literal
            && source != ty
            && !self.context.get_assignment_cast(&source, &ty).is_some()
        {
            self.context
                .report_analysis_problem(AnalysisProblem::insert(
                    crate::sem::problem::InsertProblemKind::assignment_type(),
                    &format!("cannot assign {source:?} to {ty:?}"),
                ))?;
        }
        if (value.get_type().is_some() || unknown_literal)
            && value.get_type().as_ref() != Some(&ty)
            && !value.needs_type_inference()
        {
            *value = SemScalarExpr::wrap_implicit_cast(&ty, value);
        }
        Ok(())
    }
}

/// Analyze a PostgreSQL INSERT through the common AST and ParseContext.
pub fn analyze_insert_statement<TParseContext>(
    mut context: TParseContext,
    _parent_schema: &Option<String>,
    syn: crate::syn::InsertStmt,
) -> Result<(SemAst, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    AnalysisError::check_dialect(crate::ast::Dialect::postgres(), context.dialect())?;
    let syntax = crate::ast::Insert::try_from_with_tokens(&syn, &::sql_fun_core::IVec::default())?;
    let insert = syntax.resolve(&mut context)?;
    Ok((SemAst::insert(&insert), context))
}
