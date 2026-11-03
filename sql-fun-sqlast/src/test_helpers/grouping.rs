use crate::sem::{AnalysisError, GroupingExpr};

use crate::sem::{SelectStatement, SemScalarExpr};

pub(crate) fn groupings(query: &SelectStatement) -> Vec<GroupingExpr> {
    #[derive(Default)]
    struct Groupings(Vec<GroupingExpr>);
    impl crate::sem::traversal::SemanticVisitorMut for Groupings {
        fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if let Some(grouping) = expression.as_grouping() {
                self.0.push(grouping.clone());
            }
            Ok(())
        }
    }
    let mut result = Groupings::default();
    query.clone().walk(&mut result).unwrap();
    result.0
}
