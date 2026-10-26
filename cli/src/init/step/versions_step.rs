use super::{InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct VersionsStep;
impl InitStep for VersionsStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_versions().await?;
            Ok(StepCompletion::next())
        })
    }
}
