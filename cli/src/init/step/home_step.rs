use super::{Direction, InitStep, StepFuture};
use crate::init::InitializationState;
use crate::init::{conversation::Conversation, flow};

pub(in crate::init) struct HomeStep;
impl InitStep for HomeStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move { Conversation::new(flow::home()).run(state, direction).await })
    }
}
