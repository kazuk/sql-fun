use crate::ast::{CteBody, validation::ContainsInvalidCteClause};
use crate::sem::AnalysisError;

impl CteBody {
    pub(in crate::ast::resolve) fn require_insert_placement(
        &self,
        top_level: bool,
    ) -> Result<(), AnalysisError> {
        if matches!(self, Self::Insert(_)) && !top_level {
            return Err(AnalysisError::unexpected_input_error(
                "data-modifying WITH must be at the top level",
            ));
        }
        if self.contains_invalid_cte_clause() {
            return Err(AnalysisError::unexpected_input_error(
                "nested data-modifying WITH or SELECT INTO in CTE",
            ));
        }
        Ok(())
    }
}
