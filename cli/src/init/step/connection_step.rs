use super::{InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct ConnectionStep;
impl InitStep for ConnectionStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_connection()?;
            Ok(StepCompletion::next())
        })
    }
}
