use super::{InitStep, StepFuture};
use crate::init::InitializationState;
use crate::init::{conversation::Conversation, flow};

pub(in crate::init) struct EngineSteps;
impl InitStep for EngineSteps {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move { Conversation::new(flow::engines(state)).run(state).await })
    }
}
