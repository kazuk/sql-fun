use super::{InitStep, StepFuture};
use crate::init::InitializationState;
use crate::init::{conversation::Conversation, flow};

pub(in crate::init) struct SelectionStep;
impl InitStep for SelectionStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.load_selection()?;
            let completion = Conversation::new(flow::selection(state)).run(state).await?;
            if !completion.is_cancelled() && !completion.is_back() {
                state.check_selection()?;
            }
            Ok(completion)
        })
    }
}
