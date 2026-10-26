use super::{
    InitializationState, SetupSelection,
    conversation::Conversation,
    error::InitError,
    postgres::PostgresSetup,
    prompt::{PromptRequest, PromptResponse},
};
use std::{future::Future, pin::Pin};

pub(super) type StepFuture<'a> =
    Pin<Box<dyn Future<Output = Result<StepCompletion, anyhow::Error>> + 'a>>;

/// A step changes the shared state only through accepted inputs. Ordering is
/// owned by the code assembling the conversation, never by the step or engine.
pub(super) trait InitStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StepCompletion {
    Next,
    Cancelled,
}

impl StepCompletion {
    pub(super) const fn next() -> Self {
        Self::Next
    }
    pub(super) const fn cancelled() -> Self {
        Self::Cancelled
    }
    pub(super) const fn is_cancelled(self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

/// The terminal boundary can be scripted without replacing conversation logic.
pub(super) trait PromptInput {
    fn answer(&mut self, request: &PromptRequest) -> Result<PromptResponse, InitError>;
}

pub(super) struct TerminalInput;
impl PromptInput for TerminalInput {
    fn answer(&mut self, request: &PromptRequest) -> Result<PromptResponse, InitError> {
        request.run()
    }
}

pub(super) struct HomeStep;
impl InitStep for HomeStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            super::conversation::Conversation::new(vec![
                Box::new(ConfirmInstallStep),
                Box::new(HomeDirectoryStep),
            ])
            .run(state)
            .await
        })
    }
}

struct ConfirmInstallStep;
impl InitStep for ConfirmInstallStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            loop {
                let automatic = if state.args.assume_yes() {
                    Some(PromptResponse::confirm(true))
                } else {
                    None
                };
                let request =
                    PromptRequest::confirm(include_str!("confirm_install.txt").trim_end())
                        .with_automatic(automatic);
                if let Some(confirmed) = state.input.answer(&request)?.confirmation() {
                    return Ok(if confirmed {
                        StepCompletion::next()
                    } else {
                        StepCompletion::cancelled()
                    });
                }
            }
        })
    }
}

struct HomeDirectoryStep;
impl InitStep for HomeDirectoryStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            let mut edit = state.home.begin()?;
            loop {
                let questions: Vec<Box<dyn InitStep>> = if edit {
                    vec![Box::new(EditHomeStep), Box::new(ConfirmHomeStep)]
                } else {
                    vec![Box::new(ConfirmHomeStep)]
                };
                let completion = Conversation::new(questions).run(state).await?;
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

struct EditHomeStep;
impl InitStep for EditHomeStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.home.edit_directory(state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}

struct ConfirmHomeStep;
impl InitStep for ConfirmHomeStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.home.confirm_directory(state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}

pub(super) struct SelectionStep;
impl InitStep for SelectionStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.selection = Some(SetupSelection::load(
                state.common,
                &state.args,
                state.metadata_requested,
            )?);
            let step: Box<dyn InitStep> = if state
                .selection
                .as_ref()
                .expect("loaded selection")
                .uses_datasources(&state.args)
            {
                Box::new(DatasourceSelectionStep)
            } else {
                Box::new(EngineSelectionStep)
            };
            let completion = Conversation::new(vec![step]).run(state).await?;
            if !completion.is_cancelled() {
                state
                    .selection
                    .as_ref()
                    .expect("selected setup")
                    .check_choices(&state.args)?;
            }
            Ok(completion)
        })
    }
}

struct EngineSelectionStep;
impl InitStep for EngineSelectionStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state
                .selection
                .as_mut()
                .expect("loaded selection")
                .choose_engines(&state.args, state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}

struct DatasourceSelectionStep;
impl InitStep for DatasourceSelectionStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state
                .selection
                .as_mut()
                .expect("loaded selection")
                .choose_datasources(&state.args, state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}

pub(super) struct PostgresSourceStep;
impl InitStep for PostgresSourceStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.existing_server =
                PostgresSetup::choose_source(&mut state.args, state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}

pub(super) struct ConnectionStep;
impl InitStep for ConnectionStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            PostgresSetup::choose_connection(&mut state.args, state.input.as_mut())?;
            Ok(StepCompletion::next())
        })
    }
}

pub(super) struct VersionsStep;
impl InitStep for VersionsStep {
    fn prompt<'a>(&'a mut self, state: &'a mut InitializationState<'_>) -> StepFuture<'a> {
        Box::pin(async move {
            state.versions =
                Some(PostgresSetup::choose_versions(&state.args, state.input.as_mut()).await?);
            Ok(StepCompletion::next())
        })
    }
}

#[cfg(test)]
mod tests;
