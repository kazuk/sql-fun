use crate::init::{
    InitializationState,
    prompt::PromptRequest,
    step::{Direction, InitStep, StepCompletion, StepFuture},
};

/// Final authorization is deliberately unselected on every entry.
pub(in crate::init) struct ConfirmInstallStep;
impl InitStep for ConfirmInstallStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        _direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.prepare_plans()?;
            state.validate_complete()?;
            state
                .theme
                .as_ref()
                .expect("collected theme plan")
                .review()?;
            if state.args.assume_yes() {
                return Ok(StepCompletion::next());
            }
            loop {
                let request = PromptRequest::multi_select(
                    "Initialize sql-fun development support?",
                    &["Ok".to_owned(), "Cancel".to_owned()],
                    &[],
                );
                let answer = state.input.answer(&request)?;
                if let Some(indices) = answer.selected_indices() {
                    if indices == [0] {
                        return Ok(StepCompletion::next());
                    }
                    if indices == [1] {
                        return Ok(StepCompletion::cancelled());
                    }
                    eprintln!("Choose exactly one action explicitly: Ok or Cancel.");
                }
            }
        })
    }
}
