use super::{InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;

pub(in crate::init) struct PostgresSourceStep;
impl InitStep for PostgresSourceStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_postgres_source()?;
            Ok(StepCompletion::next())
        })
    }
}
