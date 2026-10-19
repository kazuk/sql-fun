mod arbiter;
mod destination;
mod view_support;

pub use self::arbiter::{
    InsertArbiter, InsertArbiterLookup, InsertConstraintKind, InsertConstraintLookup,
    ViewSourceArbiter,
};
pub(crate) use self::destination::InsertDestination;
pub use self::view_support::{ViewInsertInfoRead, ViewInsertSupport};

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
    view_support: Option<ViewInsertSupport>,
    columns: InsertTargetCollection,
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
        columns: &InsertTargetCollection,
        source: &InsertSource,
        with: &[QueryCte],
        returning: &ResultColumnCollection,
    ) -> Self {
        Self {
            target: target.clone(),
            view_support: None,
            columns: columns.clone(),
            source: source.clone(),
            with: with.to_vec(),
            returning: returning.clone(),
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
    pub(crate) fn set_view_support(&mut self, support: Option<&ViewInsertSupport>) {
        self.view_support = support.cloned();
    }
    /// Catalog capability retained for a view destination; absent for a table.
    pub const fn view_support(&self) -> Option<&ViewInsertSupport> {
        self.view_support.as_ref()
    }
    /// Catalog identity of the destination relation (table or view).
    pub const fn target(&self) -> &TableName {
        &self.target
    }
    /// Ordered destination bindings for source values.
    pub fn columns(&self) -> &[InsertTarget] {
        self.columns.as_slice()
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
        visitor.relation_identity(self.target().full_name());
        visitor.enter_query();
        let result = self.walk_body(visitor);
        visitor.leave_query();
        result
    }
    fn walk_body<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        for entry in &mut self.with {
            entry.walk(visitor)?;
        }
        self.source.walk(visitor, self.columns.as_mut_slice())?;
        for conflict in &mut self.conflicts {
            conflict.walk(visitor)?;
        }
        self.returning.walk(visitor)
    }
}

/// Ordered INSERT targets collected while resolving a clause.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct InsertTargetCollection(Vec<InsertTarget>);
impl InsertTargetCollection {
    pub(crate) fn push(&mut self, target: InsertTarget) {
        self.0.push(target);
    }
    pub(crate) fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }
    pub(crate) fn as_slice(&self) -> &[InsertTarget] {
        &self.0
    }
    pub(crate) fn as_mut_slice(&mut self) -> &mut [InsertTarget] {
        &mut self.0
    }
    pub(crate) fn get(&self, index: usize) -> Option<&InsertTarget> {
        self.0.get(index)
    }
    fn walk<V: SemanticVisitor>(&mut self, visitor: &mut V) -> Result<(), AnalysisError> {
        for target in &mut self.0 {
            target.destination.walk_assignment_target(visitor)?;
        }
        Ok(())
    }
}

/// Resolved direct VALUES for one INSERT clause.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct InsertValueCollection(Vec<InsertValue>);
impl InsertValueCollection {
    pub(crate) fn push(&mut self, value: InsertValue) {
        self.0.push(value);
    }
    pub(crate) fn as_slice(&self) -> &[InsertValue] {
        &self.0
    }
    pub(crate) const fn len(&self) -> usize {
        self.0.len()
    }
    fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
        targets: &InsertTargetCollection,
    ) -> Result<(), AnalysisError> {
        for (index, value) in self.0.iter_mut().enumerate() {
            if let Some(target) = targets.get(index) {
                value.walk(visitor, target)?;
            } else if let Some(expression) = value.expression_mut() {
                expression.walk(visitor)?;
            }
        }
        Ok(())
    }
}

/// Values belonging to one INSERT-like clause, preserving DEFAULT VALUES.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum InsertClauseValues {
    DefaultValues,
    Values(InsertValueCollection),
}
impl InsertClauseValues {
    pub(crate) fn from_values(values: Option<InsertValueCollection>) -> Self {
        values.map_or(Self::DefaultValues, Self::Values)
    }
    pub(crate) const fn values(&self) -> Option<&InsertValueCollection> {
        match self {
            Self::DefaultValues => None,
            Self::Values(values) => Some(values),
        }
    }
    pub(crate) const fn values_mut(&mut self) -> Option<&mut InsertValueCollection> {
        match self {
            Self::DefaultValues => None,
            Self::Values(values) => Some(values),
        }
    }
}

/// Reusable resolved target and values pair for INSERT-like clauses.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct InsertClauseSemantics {
    targets: InsertTargetCollection,
    values: InsertClauseValues,
}
impl InsertClauseSemantics {
    pub(crate) fn new(targets: &InsertTargetCollection, values: &InsertClauseValues) -> Self {
        Self {
            targets: targets.clone(),
            values: values.clone(),
        }
    }
    pub(crate) const fn targets(&self) -> &InsertTargetCollection {
        &self.targets
    }
    pub(crate) const fn values(&self) -> &InsertClauseValues {
        &self.values
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
        self.targets.walk(visitor)?;
        if let Some(values) = self.values.values_mut() {
            values.walk(visitor, &self.targets)?;
        }
        Ok(())
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
    pub(crate) const fn empty_values() -> Self {
        Self::Values(Vec::new())
    }
    pub(crate) fn push_values(&mut self, values: &InsertValueCollection) {
        match self {
            Self::Values(rows) => rows.push(values.0.clone()),
            Self::DefaultValues | Self::Query(_) => {
                unreachable!("direct VALUES can only be appended to an INSERT VALUES source")
            }
        }
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
    modifiers: Vec<InsertInferenceModifiers>,
    predicate: Option<SemScalarExpr>,
    arbiter: InsertArbiterLookup,
}
impl InsertConflict {
    pub(crate) fn map_view_expressions<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.walk(visitor)
    }

    pub(crate) const fn new(
        inference: Vec<SemScalarExpr>,
        predicate: Option<SemScalarExpr>,
        constraint: Option<&str>,
    ) -> Self {
        let arbiter = InsertArbiterLookup::initial(!inference.is_empty() || constraint.is_some());
        Self {
            inference,
            modifiers: Vec::new(),
            predicate,
            arbiter,
        }
    }
    /// Resolved conflict-target expressions.
    pub fn inference(&self) -> &[SemScalarExpr] {
        &self.inference
    }
    /// Catalog lookup outcomes for each inference expression, in source order.
    pub fn modifiers(&self) -> &[InsertInferenceModifiers] {
        &self.modifiers
    }
    pub(crate) fn set_modifiers(&mut self, modifiers: Vec<InsertInferenceModifiers>) {
        self.modifiers = modifiers;
    }
    pub(crate) fn has_modifiers(&self) -> bool {
        self.modifiers
            .iter()
            .any(InsertInferenceModifiers::is_explicit)
    }
    pub(crate) fn modifiers_are_bound(&self) -> bool {
        self.modifiers
            .iter()
            .all(InsertInferenceModifiers::is_bound)
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

/// Completed collation and operator-class lookups for one inference expression.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertInferenceModifiers {
    collations: Vec<crate::sem::CatalogNameLookup>,
    operator_class: Option<crate::sem::CatalogNameLookup>,
}
impl InsertInferenceModifiers {
    #[cfg(test)]
    pub(crate) fn new(
        collation: Option<crate::sem::CatalogNameLookup>,
        operator_class: Option<crate::sem::CatalogNameLookup>,
    ) -> Self {
        Self::from_lookups(collation.into_iter().collect(), operator_class)
    }
    pub(crate) const fn from_lookups(
        collations: Vec<crate::sem::CatalogNameLookup>,
        operator_class: Option<crate::sem::CatalogNameLookup>,
    ) -> Self {
        Self {
            collations,
            operator_class,
        }
    }
    /// Effective explicit collation lookup, absent when no COLLATE was supplied.
    pub fn collation(&self) -> Option<&crate::sem::CatalogNameLookup> {
        self.collations.last()
    }
    /// All explicit COLLATE references, innermost to outermost, including names
    /// overridden by a later COLLATE. Every reference must be resolved.
    pub fn collation_references(&self) -> &[crate::sem::CatalogNameLookup] {
        &self.collations
    }
    /// Explicit operator-class lookup, absent if SQL did not specify one.
    pub const fn operator_class(&self) -> Option<&crate::sem::CatalogNameLookup> {
        self.operator_class.as_ref()
    }
    const fn is_explicit(&self) -> bool {
        !self.collations.is_empty() || self.operator_class.is_some()
    }
    fn is_bound(&self) -> bool {
        self.collations.iter().all(|v| v.resolved_name().is_some())
            && self
                .operator_class
                .as_ref()
                .is_none_or(|v| v.resolved_name().is_some())
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
            && self.context.get_assignment_cast(&source, &ty).is_none()
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
    Ok((
        insert
            .as_ref()
            .map_or_else(SemAst::unresolved_statement, SemAst::insert),
        context,
    ))
}
