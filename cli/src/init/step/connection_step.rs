use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct ConnectionStep;
impl InitStep for ConnectionStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_connection()?;
            Ok(direction.completed(state.args.database_url().is_some()))
        })
    }
}
