use crate::sem::{
    AlterObjSubCommand, AnalysisError, AnalysisProblem, CreateView, ParseContext,
    problem::ViewProblem,
};
impl AlterObjSubCommand {
    pub(crate) fn validate_for_view(
        &self,
        context: &mut impl ParseContext,
        view: &CreateView,
    ) -> Result<bool, AnalysisError> {
        let name = match self {
            Self::SetColumnDefault(name, _) | Self::DropColumnDefault(name) => name,
            _ => return Ok(true),
        };
        if view.has_column(name) {
            return Ok(true);
        }
        context.report_analysis_problem(AnalysisProblem::view(
            &ViewProblem::unknown_default_column(view.name(), name.as_str()),
        ))?;
        Ok(false)
    }
    pub(crate) fn apply_to_view(&self, view: &mut CreateView) {
        match self {
            Self::ChangeOwner(owner) => {
                if owner.can_apply() {
                    view.change_owner(owner);
                }
            }
            Self::SetColumnDefault(name, value) => view
                .change_column_default(name, Some(value))
                .expect("validated default column"),
            Self::DropColumnDefault(name) => view
                .change_column_default(name, None)
                .expect("validated default column"),
            _ => todo!(),
        }
    }
}
