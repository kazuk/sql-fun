//! Query-local GROUPING bindings. The shared environment remains ParseContext.

use crate::{
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, QueryGrouping, QueryProblem, ResultColumnCollection,
        SelectStatement, SemScalarExpr,
        traversal::{SemanticVisitor, SemanticVisitorMut},
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
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for argument in &mut self.arguments {
            argument.expression.walk(visitor)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_retains_type_nullability_and_parameter_operands {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::{GroupingExpr, PgBuiltInType},
        test_helpers::{grouping::groupings, query::resolve},
    };

    #[test]
    fn retains_type_nullability_and_parameter_operands() {
        let (query, context) = resolve(
            "SELECT GROUPING(a+$7) FROM (SELECT 1 AS a) s GROUP BY a+$7",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let expression = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap();
        assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(expression.is_not_null(), Some(true));
        assert_eq!(expression.get_column_name(), "grouping");
        let parameter = context.parameter_constraints();
        let constraint = parameter.parameter(7).unwrap().constraint();
        assert!(constraint.allows(&PgBuiltInType::int4()));
        assert!(!constraint.allows(&PgBuiltInType::text()));
        let grouping = expression.as_grouping().unwrap();
        assert!(grouping.is_valid());
        assert_eq!(
            grouping.arguments()[0].expression().get_type(),
            Some(PgBuiltInType::int4())
        );
    }

    #[test]
    fn grouping_result_type_does_not_constrain_argument_type() {
        let (query, context) = resolve(
            "SELECT GROUPING(CAST($1 AS text)) GROUP BY CAST($1 AS text)",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let grouping = groupings(&query).pop().unwrap();
        assert!(grouping.is_valid());
        assert_eq!(
            grouping.arguments()[0].expression().get_type(),
            Some(PgBuiltInType::text())
        );
        let serialized = ::serde_json::to_string(&grouping).unwrap();
        let restored: GroupingExpr = ::serde_json::from_str(&serialized).unwrap();
        assert!(restored.is_valid());
        assert_eq!(restored.arguments()[0].key_index(), Some(0));
    }
}

impl GroupingExpr {
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for argument in &self.arguments {
            argument.expression.visit(visitor)?;
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

#[cfg(test)]
mod test_binds_grouping_keys {
    use crate::{
        ast::{Dialect, StatementNode},
        parse_statements,
        sem::{SelectStatement, SemScalarExpr},
        test_helpers::{
            grouping::groupings,
            query::{catalog, resolve},
        },
    };

    #[rstest::rstest]
    #[case::ordinary("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::rollup("SELECT GROUPING(b,a,b) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY ROLLUP(a,b)", &[1,0,1])]
    #[case::cube("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY CUBE(a,b)", &[1,0])]
    #[case::sets("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY GROUPING SETS ((a,b),(),(a))", &[1,0])]
    #[case::duplicate_keys("SELECT GROUPING(a,a) FROM (SELECT 1 AS a) s GROUP BY a,a", &[0,0])]
    #[case::later_parameter("SELECT GROUPING(a+1), $2 FROM (SELECT 1 AS a) s GROUP BY a+1", &[0])]
    #[case::qualified_function("SELECT GROUPING(identity(a)) FROM (SELECT 1 AS a) s GROUP BY public.identity(a)", &[0])]
    #[case::repeated_before_another_key("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY a,a,b", &[2,0])]
    #[case::expression("SELECT GROUPING(s.a+1) FROM (SELECT 1 AS a) s GROUP BY a+1", &[0])]
    #[case::alias("SELECT a+1 AS x, GROUPING(a+1) FROM (SELECT 1 AS a) s GROUP BY x", &[0])]
    #[case::ordinal("SELECT a+1 AS x, GROUPING(a+1) FROM (SELECT 1 AS a) s GROUP BY 1", &[0])]
    #[case::input_precedence("SELECT a+1 AS a, GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::coalesce("SELECT COALESCE(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::nested_local_grouping("SELECT sum((SELECT GROUPING(t.a) FROM (SELECT 2 AS a) t GROUP BY t.a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::group_alias_local_subquery("SELECT (SELECT GROUPING(t.a) FROM (SELECT 2 AS a) t GROUP BY t.a) AS g GROUP BY g", &[0])]
    #[case::constant("SELECT GROUPING(1+1) GROUP BY (1+1)", &[0])]
    #[case::parentheses("SELECT GROUPING((a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::noop_argument_cast("SELECT GROUPING(CAST(a AS integer)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::noop_key_cast("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY CAST(a AS integer)", &[0])]
    #[case::having("SELECT a FROM (SELECT 1 AS a) s GROUP BY a HAVING GROUPING(a) IS NOT NULL", &[0])]
    #[case::order("SELECT a FROM (SELECT 1 AS a) s GROUP BY a ORDER BY GROUPING(a)", &[0])]
    #[case::window_order("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a WINDOW w AS (ORDER BY GROUPING(a))", &[0])]
    #[case::window_aggregate("SELECT sum(GROUPING(a)) OVER () FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    fn binds_grouping_keys(
        #[case] sql: &str,
        #[case] indices: &[usize],
        #[values(false, true)] infer: bool,
    ) {
        let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{sql}: {:?}",
            context.reported_problems()
        );
        let groupings = groupings(&query);
        assert!(!groupings.is_empty());
        for grouping in groupings {
            assert!(grouping.is_valid(), "{sql}: {grouping:?}");
            assert_eq!(grouping.levels_up(), 0);
            assert_eq!(grouping.arguments().len(), indices.len());
            for (argument, index) in grouping.arguments().iter().zip(indices) {
                assert_eq!(argument.key_index(), Some(*index));
            }
        }
    }

    #[rstest::rstest]
    #[case::missing("SELECT GROUPING(missing) GROUP BY missing", false)]
    #[case::ambiguous(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s, (SELECT 2 AS a) t GROUP BY s.a",
        true
    )]
    fn preserves_failed_argument_lookup(#[case] sql: &str, #[case] ambiguous: bool) {
        let (query, _) = resolve(sql, Dialect::postgres(), false).unwrap();
        let grouping = groupings(&query).pop().unwrap();
        assert!(!grouping.is_valid());
        assert_eq!(grouping.arguments()[0].key_index(), None);
        let expression = grouping.arguments()[0].expression();
        if ambiguous {
            ::assert_matches::assert_matches!(
                expression,
                SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::AmbiguousColumnRef(_))
            );
        } else {
            ::assert_matches::assert_matches!(
                expression,
                SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::UndefinedColumn(_))
            );
        }
    }

    #[test]
    fn rejects_invalid_grouping_through_diagnostic_policy() {
        let statement = parse_statements("SELECT GROUPING(1)", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::Query(query) = statement.node() else {
            panic!("query");
        };
        let mut context = catalog();
        context.reject_diagnostics_after(0);
        assert!(
            SelectStatement::from_ast(
                query,
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_err()
        );
        assert_eq!(context.reported_problem_count(), 0);
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
impl<C: ParseContext> SemanticVisitorMut for GroupingKeyValidation<'_, C> {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.validate_grouping_key_node(self.query_id, self.context)
    }
}
struct GroupingCompletion<'a, C> {
    context: &'a mut C,
}
impl<C: ParseContext> SemanticVisitorMut for GroupingCompletion<'_, C> {
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

#[cfg_attr(test, derive(Clone))]
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
impl SemanticVisitorMut for GroupingCastNormalization {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.normalize_grouping_cast();
        Ok(())
    }
}
struct ComparisonKey {
    resolved: bool,
}
impl SemanticVisitorMut for ComparisonKey {
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

#[cfg(test)]
mod test_binds_the_owning_query {
    use crate::{
        ast::Dialect,
        test_helpers::{grouping::groupings, query::resolve},
    };

    #[rstest::rstest]
    #[case::outer("SELECT (SELECT GROUPING(s.a)) FROM (SELECT 1 AS a) s GROUP BY a", 1)]
    #[case::shadowed(
        "SELECT (SELECT GROUPING(s.a) FROM (SELECT 2 AS a) s GROUP BY s.a) FROM (SELECT 1 AS a) s GROUP BY a",
        0
    )]
    #[case::mixed(
        "SELECT (SELECT GROUPING(s.a+t.a) FROM (SELECT 2 AS a) t GROUP BY s.a+t.a) FROM (SELECT 1 AS a) s GROUP BY a",
        0
    )]
    fn binds_the_owning_query(#[case] sql: &str, #[case] levels_up: usize) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        for grouping in groupings(&query) {
            assert!(grouping.is_valid(), "{grouping:?}");
            assert_eq!(grouping.levels_up(), levels_up);
            assert_eq!(grouping.arguments()[0].key_index(), Some(0));
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
