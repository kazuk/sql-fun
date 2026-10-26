use super::{InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct SqliteStep;
impl InitStep for SqliteStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.prepare_sqlite();
            Ok(StepCompletion::next())
        })
    }
}
