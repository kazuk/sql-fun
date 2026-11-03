use crate::init::{
    InitializationState,
    conversation::Conversation,
    flow,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct SelectionStep;
impl InitStep for SelectionStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.load_selection()?;
            let completion = Conversation::new(flow::selection(state))
                .run(state, direction)
                .await?;
            if !completion.is_cancelled() && !completion.is_back() {
                state.check_selection()?;
            }
            Ok(completion)
        })
    }
}
