use crate::{
    context::ParseContext,
    sem::{AnalysisProblem, CatalogNameLookup, ResolutionFailure, problem::InsertProblemKind},
};

impl CatalogNameLookup {
    pub(crate) fn report_insert_problem(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<(), ResolutionFailure> {
        let kind = if self.is_missing() {
            Some(InsertProblemKind::conflict_name_missing())
        } else if self.ambiguous_candidates().is_some() {
            Some(InsertProblemKind::conflict_name_ambiguous())
        } else {
            None
        };
        if let Some(kind) = kind {
            context.report_analysis_problem(AnalysisProblem::insert(
                kind,
                &format!(
                    "conflict target catalog name {} has no unique binding",
                    self.requested()
                ),
            ))?;
        }
        Ok(())
    }
}
