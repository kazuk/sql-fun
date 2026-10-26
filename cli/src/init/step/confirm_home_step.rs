use super::{InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct ConfirmHomeStep;
impl InitStep for ConfirmHomeStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.home.confirm_directory(state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}
