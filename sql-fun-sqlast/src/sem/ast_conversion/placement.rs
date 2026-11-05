use crate::ast::{CteBody, validation::ContainsInvalidCteClause};
use crate::sem::CteStatement;
use crate::{
    context::ParseContext,
    sem::{AnalysisError, AnalysisProblem, problem::QueryProblem},
};

impl CteStatement {
    pub(crate) fn ast_require_data_modifying_placement(
        ast: &CteBody,
        context: &mut impl ParseContext,
        top_level: bool,
    ) -> Result<(), AnalysisError> {
        if (matches!(
            ast,
            CteBody::Insert(_) | CteBody::Update(_) | CteBody::Delete(_) | CteBody::Merge(_)
        ) && !top_level)
            || ast.contains_invalid_cte_clause()
        {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_cte_placement(),
            ))?;
        }
        Ok(())
    }
}
