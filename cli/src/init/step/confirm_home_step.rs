use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct ConfirmHomeStep;
impl InitStep for ConfirmHomeStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.home.confirm_directory(state.input.as_mut())?;
            Ok(direction.completed(state.args.assume_yes()))
        })
    }
}
