use super::{InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;
use crate::init::{conversation::Conversation, flow};

pub(in crate::init) struct HomeDirectoryStep;
impl InitStep for HomeDirectoryStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            let mut edit = state.home.begin()?;
            loop {
                let completion = Conversation::new(flow::home_directory(edit))
                    .run(state)
                    .await?;
                if completion.is_cancelled() {
                    return Ok(completion);
                }
                if state.home.confirmed() && state.home.create_directory()? {
                    return Ok(StepCompletion::next());
                }
                edit = true;
            }
        })
    }
}
