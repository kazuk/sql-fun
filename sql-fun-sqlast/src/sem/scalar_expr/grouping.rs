//! Query-local GROUPING bindings. The shared environment remains ParseContext.
use crate::{
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, QueryGrouping, QueryProblem, ResultColumnCollection,
        SelectStatement, SemScalarExpr, traversal::SemanticVisitor,
    },
};

use std::{cell::RefCell, rc::Rc};

/// One analyzed GROUPING argument and its completed grouping-key lookup.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GroupingArgument {
    expression: SemScalarExpr,
    key_index: Option<usize>,
}
impl GroupingArgument {
    /// Resolved expression. GROUPING analyzes but does not evaluate this operand.
    pub const fn expression(&self) -> &SemScalarExpr {
        &self.expression
    }
    /// Zero-based expression position in the owning SELECT's GROUP BY tree,
    /// flattened in depth-first declaration order (empty sets have no position).
    /// Equivalent repeated keys bind to their first occurrence. None denotes a
    /// failed lookup. GROUP BY aliases/ordinals identify their projection target.
    pub const fn key_index(&self) -> Option<usize> {
        self.key_index
    }
}

/// PostgreSQL GROUPING operation, preserving argument order and duplicate bits.
///
/// The enclosing scalar expression reports PostgreSQL `int4` and non-null
/// result metadata. Invalid operations remain inspectable: failed matches have
/// no key index, `is_valid()` is false, and the analysis context reports query
/// diagnostics for matching, placement/nesting, or the 31-argument limit.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GroupingExpr {
    arguments: Vec<GroupingArgument>,
    #[serde(skip)]
    query_id: Option<usize>,
    levels_up: usize,
    valid: bool,
    completed: bool,
}
impl GroupingExpr {
    pub(crate) fn new(
        arguments: &[SemScalarExpr],
        query_id: Option<usize>,
        levels_up: usize,
        valid: bool,
    ) -> Self {
        Self {
            arguments: arguments
                .iter()
                .cloned()
                .map(GroupingArgument::from)
                .collect(),
            query_id,
            levels_up,
            valid,
            completed: false,
        }
    }
    /// Ordered operands; the rightmost argument supplies the least-significant bit.
    pub fn arguments(&self) -> &[GroupingArgument] {
        &self.arguments
    }
    /// Number of enclosing SQL query levels between this occurrence and its owner.
    pub const fn levels_up(&self) -> usize {
        self.levels_up
    }
    /// Whether argument matching and placement/nesting validation succeeded.
    pub const fn is_valid(&self) -> bool {
        self.valid && self.completed
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for argument in &mut self.arguments {
            argument.expression.walk(visitor)?;
        }
        Ok(())
    }
    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.query_id = None;
    }
    fn reject_key(
        &mut self,
        query_id: usize,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if self.query_id == Some(query_id) {
            self.valid = false;
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_grouping_placement("GROUP BY"),
            ))?;
        }
        Ok(())
    }
    fn complete(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        if self.completed {
            return Ok(());
        }
        let Some(id) = self.query_id else {
            return Ok(());
        };
        let Some(Some(keys)) = context.analysis_state().grouping().keys.get(id) else {
            return Ok(());
        };
        let keys = keys.clone();
        for argument in &mut self.arguments {
            argument.key_index = if let Some(key) = comparison_key(&argument.expression)? {
                keys.iter()
                    .position(|candidate| candidate.as_ref() == Some(&key))
            } else {
                None
            };
            if argument.key_index.is_none() {
                self.valid = false;
                context.report_analysis_problem(AnalysisProblem::query(
                    QueryProblem::grouping_argument_not_grouped(),
                ))?;
            }
        }
        self.completed = true;
        Ok(())
    }
}
impl From<SemScalarExpr> for GroupingArgument {
    fn from(expression: SemScalarExpr) -> Self {
        Self {
            expression,
            key_index: None,
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn reject_grouping_key(
        &mut self,
        query_id: usize,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.walk(&mut GroupingKeyValidation { query_id, context })
    }

    fn validate_grouping_key_node(
        &mut self,
        query_id: usize,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Self::Grouping(grouping) = self {
            grouping.reject_key(query_id, context)?;
        }
        Ok(())
    }

    pub(crate) fn grouping(
        arguments: &[Self],
        owner: Option<usize>,
        levels_up: usize,
        valid: bool,
    ) -> Self {
        Self::Grouping(GroupingExpr::new(arguments, owner, levels_up, valid))
    }
    /// GROUPING semantics, when this expression is a grouping operation.
    pub const fn as_grouping(&self) -> Option<&GroupingExpr> {
        if let Self::Grouping(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn complete_grouping_node(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Self::Grouping(grouping) = self {
            grouping.complete(context)?;
        }
        Ok(())
    }
    pub(crate) fn complete_grouping(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if !context.analysis_state().grouping().has_operations {
            return Ok(());
        }
        self.walk(&mut GroupingCompletion { context })
    }
}
struct GroupingKeyValidation<'a, C> {
    query_id: usize,
    context: &'a mut C,
}
impl<C: ParseContext> SemanticVisitor for GroupingKeyValidation<'_, C> {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.validate_grouping_key_node(self.query_id, self.context)
    }
}
struct GroupingCompletion<'a, C> {
    context: &'a mut C,
}
impl<C: ParseContext> SemanticVisitor for GroupingCompletion<'_, C> {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.complete_grouping_node(self.context)
    }
}
impl SelectStatement {
    pub(crate) fn complete_grouping(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if !context.analysis_state().grouping().has_operations {
            return Ok(());
        }
        self.walk(&mut GroupingCompletion { context })
    }
}

#[derive(Debug, Default)]
pub(crate) struct GroupingAnalysis {
    has_operations: bool,
    keys: Vec<Option<Vec<Option<SemScalarExpr>>>>,
}
impl GroupingAnalysis {
    pub(crate) const fn register_operation(&mut self) {
        self.has_operations = true;
    }

    pub(crate) fn new_query(&mut self) -> usize {
        let id = self.keys.len();
        self.keys.push(None);
        id
    }
    pub(crate) fn set_keys(
        &mut self,
        id: usize,
        groups: &[QueryGrouping],
        columns: &ResultColumnCollection,
    ) -> Result<(), AnalysisError> {
        let mut keys = Vec::new();
        for group in groups {
            group.collect_grouping_keys(columns, &mut keys)?;
        }
        self.keys[id] = Some(keys);
        Ok(())
    }
}
impl QueryGrouping {
    pub(crate) fn validate_grouping_targets(
        &self,
        columns: &mut ResultColumnCollection,
        query_id: usize,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(expression) => {
                Self::validate_expression_target(expression, columns, query_id, context)
            }
            Self::Empty => Ok(()),
            Self::Rollup(groups)
            | Self::Cube(groups)
            | Self::Sets(groups)
            | Self::Simple(groups) => {
                Self::validate_target_list(groups, columns, query_id, context)
            }
        }
    }

    fn validate_expression_target(
        expression: &SemScalarExpr,
        columns: &mut ResultColumnCollection,
        query_id: usize,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Some(index) = expression.output_index() {
            columns.validate_grouping_target(index, query_id, context)?;
        }
        Ok(())
    }

    fn validate_target_list(
        groups: &[Self],
        columns: &mut ResultColumnCollection,
        query_id: usize,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        for group in groups {
            group.validate_grouping_targets(columns, query_id, context)?;
        }
        Ok(())
    }

    fn collect_grouping_keys(
        &self,
        columns: &ResultColumnCollection,
        keys: &mut Vec<Option<SemScalarExpr>>,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(expression) => Self::collect_expression_key(expression, columns, keys),
            Self::Empty => Ok(()),
            Self::Rollup(groups)
            | Self::Cube(groups)
            | Self::Sets(groups)
            | Self::Simple(groups) => Self::collect_key_list(groups, columns, keys),
        }
    }

    fn collect_expression_key(
        expression: &SemScalarExpr,
        columns: &ResultColumnCollection,
        keys: &mut Vec<Option<SemScalarExpr>>,
    ) -> Result<(), AnalysisError> {
        let expression = if let Some(index) = expression.output_index() {
            columns.grouping_expression(index).unwrap_or(expression)
        } else {
            expression
        };
        keys.push(comparison_key(expression)?);
        Ok(())
    }

    fn collect_key_list(
        groups: &[Self],
        columns: &ResultColumnCollection,
        keys: &mut Vec<Option<SemScalarExpr>>,
    ) -> Result<(), AnalysisError> {
        for group in groups {
            group.collect_grouping_keys(columns, keys)?;
        }
        Ok(())
    }
}

// Compare canonical semantic expressions, normalizing known no-op casts and
// removing transient inference IDs and caches. Names and remaining casts retain
// their resolved identity.
fn comparison_key(expression: &SemScalarExpr) -> Result<Option<SemScalarExpr>, AnalysisError> {
    let mut expression = expression.clone();
    // Remove known no-op casts while column type snapshots are still available.
    expression.walk(&mut GroupingCastNormalization)?;
    let mut canonicalizer = ComparisonKey { resolved: true };
    expression.walk(&mut canonicalizer)?;
    if canonicalizer.resolved {
        Ok(Some(expression))
    } else {
        Ok(None)
    }
}
struct GroupingCastNormalization;
impl SemanticVisitor for GroupingCastNormalization {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.normalize_grouping_cast();
        Ok(())
    }
}
struct ComparisonKey {
    resolved: bool,
}
impl SemanticVisitor for ComparisonKey {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        self.resolved &= expression.grouping_references_resolved();
        expression.canonicalize_grouping_key();
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct QueryReference {
    id: usize,
    depth: usize,
    placement: String,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct ReferenceCapture(Rc<RefCell<CapturedReferences>>);
#[derive(Debug, Default)]
struct CapturedReferences {
    columns: Vec<QueryReference>,
    aggregates: Vec<QueryReference>,
    windows: Vec<usize>,
}
impl ReferenceCapture {
    pub(crate) fn invalid_nesting(&self, owner: Option<usize>, scope: &GroupingScope) -> bool {
        let references = self.0.borrow();
        references
            .aggregates
            .iter()
            .any(|reference| Some(reference.id) == owner)
            || references.windows.contains(&scope.depth)
    }

    pub(crate) fn owner(&self, scope: &GroupingScope) -> (Option<usize>, usize, String) {
        let references = self.0.borrow();
        let owner = references
            .columns
            .iter()
            .chain(&references.aggregates)
            .filter(|reference| reference.depth <= scope.depth)
            .max_by_key(|reference| reference.depth);
        if let Some(owner) = owner {
            (
                Some(owner.id),
                scope.depth - owner.depth,
                if Some(owner.id) == scope.query_id {
                    scope.placement.clone()
                } else {
                    owner.placement.clone()
                },
            )
        } else {
            (scope.query_id, 0, scope.placement.clone())
        }
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct GroupingScope {
    query_id: Option<usize>,
    depth: usize,
    placement: String,
    captures: Vec<ReferenceCapture>,
}
impl GroupingScope {
    pub(crate) const fn new() -> Self {
        Self {
            query_id: None,
            depth: 0,
            placement: String::new(),
            captures: Vec::new(),
        }
    }

    pub(crate) fn nested(parent: Option<&Self>) -> Self {
        Self {
            depth: parent.map_or(0, |scope| scope.depth + 1),
            captures: parent.map_or_else(Vec::new, |scope| scope.captures.clone()),
            ..Self::default()
        }
    }
    pub(crate) const fn set_query(&mut self, id: usize) {
        self.query_id = Some(id);
    }
    pub(crate) fn set_placement(&mut self, placement: &str) {
        self.placement = placement.to_owned();
    }
    pub(crate) fn add_capture(&mut self, capture: &ReferenceCapture) {
        self.captures.push(capture.clone());
    }
    pub(crate) fn aggregate(&self, owner: Option<usize>, levels_up: usize) {
        if let Some(id) = owner {
            for capture in &self.captures {
                capture.0.borrow_mut().aggregates.push(QueryReference {
                    id,
                    depth: self.depth - levels_up,
                    placement: self.placement.clone(),
                });
            }
        }
    }
    pub(crate) fn window(&self) {
        for capture in &self.captures {
            capture.0.borrow_mut().windows.push(self.depth);
        }
    }
    pub(crate) fn reference(&self) {
        if let Some(id) = self.query_id {
            for capture in &self.captures {
                capture.0.borrow_mut().columns.push(QueryReference {
                    id,
                    depth: self.depth,
                    placement: self.placement.clone(),
                });
            }
        }
    }
}
