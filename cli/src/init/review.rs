//! Final review uses the same owners and prepared plans as installation.
use crate::init::{EngineSetup, InitializationState, selection_presentation::SQLITE_DESCRIPTION};
use std::path::Path;

impl InitializationState<'_> {
    pub(super) fn review(&self) -> Result<String, anyhow::Error> {
        let mut lines = vec![
            "Review setup — no installation has started.".to_owned(),
            self.home.review(),
            self.selection
                .as_ref()
                .expect("selected setup")
                .review(self.common),
        ];
        lines.extend(
            self.plans
                .iter()
                .map(|plan| plan.review(self.home.home(), self.args.database_url().is_some())),
        );
        lines.push(
            self.theme
                .as_ref()
                .expect("collected theme plan")
                .review()?,
        );
        lines.push("After Ok: create/reuse the home, apply the theme operation, then process selected engine setup in the displayed order. Failure can leave the home, theme, and completed versions in place. Existing user data is not removed as cancellation cleanup.\nCancel exits without installing. Esc returns to the preceding question.".to_owned());
        Ok(lines.join("\n"))
    }
}

impl EngineSetup {
    fn review(&self, home: &Path, supplied_connection: bool) -> String {
        match self {
            Self::Postgres(setup) => setup.review(home, supplied_connection),
            Self::Sqlite => SQLITE_DESCRIPTION.to_owned(),
        }
    }
}
