use super::{Direction, InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct EditHomeStep;
impl InitStep for EditHomeStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        _direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.home.edit_directory(state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}
