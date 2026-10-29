use super::{Direction, InitStep, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct DatasourceSelectionStep;
impl InitStep for DatasourceSelectionStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_datasources()?;
            Ok(direction.completed(state.args.datasources().is_some()))
        })
    }
}
