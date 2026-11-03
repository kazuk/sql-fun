use crate::init::{
    InitializationState,
    conversation::Conversation,
    flow,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct PostgresSteps;
impl InitStep for PostgresSteps {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            Conversation::new(flow::postgres())
                .run(state, direction)
                .await
        })
    }
}
