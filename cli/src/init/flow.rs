use super::{
    EngineSetup, InitializationState, SetupSelection,
    conversation::Conversation,
    step::{
        ConnectionStep, HomeStep, InitStep, PostgresSourceStep, SelectionStep, StepCompletion,
        StepFuture, VersionsStep,
    },
};

/// This configuration, rather than the engine, owns question order and branches.
pub(super) fn conversation(yes: bool) -> Conversation {
    if yes {
        Conversation::new(vec![
            Box::new(SelectionStep),
            Box::new(EngineSteps),
            Box::new(HomeStep),
        ])
    } else {
        Conversation::new(vec![
            Box::new(HomeStep),
            Box::new(SelectionStep),
            Box::new(EngineSteps),
        ])
    }
}

pub(super) async fn collect(
    state: &mut InitializationState<'_>,
) -> Result<StepCompletion, anyhow::Error> {
    let completion = conversation(state.args.assume_yes()).run(state).await?;
    if !completion.is_cancelled() {
        state.validate_complete()?;
    }
    Ok(completion)
}

struct EngineSteps;
impl InitStep for EngineSteps {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state
                .selection
                .as_ref()
                .expect("selected setup")
                .conversation()
                .run(state)
                .await
        })
    }
}

impl SetupSelection {
    fn conversation(&self) -> Conversation {
        let mut steps: Vec<Box<dyn InitStep>> = Vec::new();
        for engine in &self.engines {
            if engine == "postgres" {
                steps.push(Box::new(PostgresSteps));
            } else {
                steps.push(Box::new(SqliteStep));
            }
        }
        Conversation::new(steps)
    }
}

struct PostgresSteps;
impl InitStep for PostgresSteps {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            let completion =
                Conversation::new(vec![Box::new(PostgresSourceStep), Box::new(SourceDetails)])
                    .run(state)
                    .await?;
            if !completion.is_cancelled() {
                state.prepare_postgres()?;
            }
            Ok(completion)
        })
    }
}

struct SourceDetails;
impl InitStep for SourceDetails {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            let steps: Vec<Box<dyn InitStep>> = if state.existing_server {
                vec![Box::new(ConnectionStep), Box::new(VersionsStep)]
            } else {
                vec![Box::new(VersionsStep)]
            };
            Conversation::new(steps).run(state).await
        })
    }
}

struct SqliteStep;
impl InitStep for SqliteStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.plans.push(EngineSetup::sqlite());
            Ok(StepCompletion::next())
        })
    }
}
