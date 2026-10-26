use super::{InitStep, StepFuture};
use crate::init::InitializationState;
use crate::init::{conversation::Conversation, flow};

pub(in crate::init) struct PostgresSteps;
impl InitStep for PostgresSteps {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            let completion = Conversation::new(flow::postgres()).run(state).await?;
            if !completion.is_cancelled() {
                state.prepare_postgres()?;
            }
            Ok(completion)
        })
    }
}
