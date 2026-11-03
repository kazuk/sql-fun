use crate::init::{
    InitializationState,
    conversation::Conversation,
    flow,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct SourceDetails;
impl InitStep for SourceDetails {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            Conversation::new(flow::source_details(state))
                .run(state, direction)
                .await
        })
    }
}
