use super::{Direction, InitStep, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct VersionsStep;
impl InitStep for VersionsStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_versions().await?;
            Ok(direction.completed(state.args.requested_versions().is_some()))
        })
    }
}
