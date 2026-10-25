use crate::ast::{CteBody, validation::ContainsInvalidCteClause};
use crate::{
    context::ParseContext,
    sem::{AnalysisError, AnalysisProblem, problem::QueryProblem},
};

impl CteBody {
    pub(in crate::ast::resolve) fn require_data_modifying_placement(
        &self,
        context: &mut impl ParseContext,
        top_level: bool,
    ) -> Result<(), AnalysisError> {
        if (matches!(
            self,
            Self::Insert(_) | Self::Update(_) | Self::Delete(_) | Self::Merge(_)
        ) && !top_level)
            || self.contains_invalid_cte_clause()
        {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_cte_placement(),
            ))?;
        }
        Ok(())
    }
}
