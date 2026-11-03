use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct PostgresSourceStep;
impl InitStep for PostgresSourceStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state.choose_postgres_source()?;
            Ok(direction.completed(state.args.docker() || state.args.database_url().is_some()))
        })
    }
}
