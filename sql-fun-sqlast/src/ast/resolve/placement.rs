use crate::ast::{CteBody, validation::ContainsInvalidCteClause};
use crate::sem::AnalysisError;

impl CteBody {
    pub(in crate::ast::resolve) fn require_data_modifying_placement(
        &self,
        context: &mut impl crate::sem::ParseContext,
        top_level: bool,
    ) -> Result<(), AnalysisError> {
        if (matches!(
            self,
            Self::Insert(_) | Self::Update(_) | Self::Delete(_) | Self::Merge(_)
        ) && !top_level)
            || self.contains_invalid_cte_clause()
        {
            context.report_analysis_problem(crate::sem::AnalysisProblem::query(
                crate::sem::problem::QueryProblem::invalid_cte_placement(),
            ))?;
        }
        Ok(())
    }
}
