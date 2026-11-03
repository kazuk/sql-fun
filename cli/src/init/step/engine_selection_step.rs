use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct EngineSelectionStep;
impl InitStep for EngineSelectionStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_engines()?;
            Ok(direction.completed(state.args.engines().is_some()))
        })
    }
}
