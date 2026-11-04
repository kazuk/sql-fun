use crate::ast::{CteBody, validation::ContainsInvalidCteClause};
use crate::sem::AstConversion;
use crate::{
    context::ParseContext,
    sem::{AnalysisError, AnalysisProblem, problem::QueryProblem},
};

impl AstConversion<'_, CteBody> {
    pub(crate) fn require_data_modifying_placement(
        &self,
        context: &mut impl ParseContext,
        top_level: bool,
    ) -> Result<(), AnalysisError> {
        if (matches!(
            self.ast,
            CteBody::Insert(_) | CteBody::Update(_) | CteBody::Delete(_) | CteBody::Merge(_)
        ) && !top_level)
            || self.ast.contains_invalid_cte_clause()
        {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_cte_placement(),
            ))?;
        }
        Ok(())
    }
}
