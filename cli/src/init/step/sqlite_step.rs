use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct SqliteStep;
impl InitStep for SqliteStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            let _ = state;
            Ok(direction.completion())
        })
    }
}
