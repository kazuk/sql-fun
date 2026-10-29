//! Composition and ordering for initialization conversations.
//!
//! To add a step, implement `InitStep` in a new module under `step/`, declare and
//! re-export it from `step.rs`, then register it in the appropriate ordered list
//! factory here. For a new nested branch, define its list factory here and have
//! the containing step await a `Conversation` built from that list using the same
//! shared state. Keep behavior in the step module and composition in this module;
//! the generic conversation runner does not need to change.
//!
//! To retire a step, remove its registrations here and its module declaration,
//! re-export, and implementation under `step/`. Preserve the intended question
//! order and side-effect timing when updating lists, and adjust the affected
//! semantic tests.

use super::{
    InitializationState,
    conversation::Conversation,
    step::{
        ConfirmHomeStep, ConfirmInstallStep, ConnectionStep, DatasourceSelectionStep, EditHomeStep,
        EngineSelectionStep, EngineSteps, HomeDirectoryStep, HomeStep, InitStep,
        PostgresSourceStep, PostgresSteps, SelectionStep, SourceDetails, SqliteStep,
        StepCompletion, VersionsStep,
    },
};

/// All production step lists, including nested branches, are configured here.
pub(super) fn conversation(yes: bool) -> Conversation {
    let steps: Vec<Box<dyn InitStep>> = if yes {
        vec![
            Box::new(SelectionStep),
            Box::new(EngineSteps),
            Box::new(HomeStep),
            Box::new(ConfirmInstallStep),
        ]
    } else {
        vec![
            Box::new(HomeStep),
            Box::new(SelectionStep),
            Box::new(EngineSteps),
            Box::new(ConfirmInstallStep),
        ]
    };
    Conversation::new(steps)
}

pub(super) async fn collect(
    state: &mut InitializationState<'_>,
) -> Result<StepCompletion, anyhow::Error> {
    let mut conversation = conversation(state.args.assume_yes());
    loop {
        let completion = conversation.run(state).await?;
        if !completion.is_back() {
            return Ok(completion);
        }
        // Only the root absorbs Back at the first visible question.
        state.backward = false;
    }
}

pub(super) fn home() -> Vec<Box<dyn InitStep>> {
    vec![Box::new(HomeDirectoryStep)]
}

pub(super) fn home_directory(edit: bool) -> Vec<Box<dyn InitStep>> {
    if edit {
        vec![Box::new(EditHomeStep), Box::new(ConfirmHomeStep)]
    } else {
        vec![Box::new(ConfirmHomeStep)]
    }
}

pub(super) fn selection(state: &InitializationState<'_>) -> Vec<Box<dyn InitStep>> {
    if state
        .selection
        .as_ref()
        .expect("loaded selection")
        .uses_datasources(&state.args)
    {
        vec![Box::new(DatasourceSelectionStep)]
    } else {
        vec![Box::new(EngineSelectionStep)]
    }
}

pub(super) fn engines(state: &InitializationState<'_>) -> Vec<Box<dyn InitStep>> {
    let mut steps: Vec<Box<dyn InitStep>> = Vec::new();
    for engine in &state.selection.as_ref().expect("selected setup").engines {
        if engine == "postgres" {
            steps.push(Box::new(PostgresSteps));
        } else {
            steps.push(Box::new(SqliteStep));
        }
    }
    steps
}

pub(super) fn postgres() -> Vec<Box<dyn InitStep>> {
    vec![Box::new(PostgresSourceStep), Box::new(SourceDetails)]
}

pub(super) fn source_details(state: &InitializationState<'_>) -> Vec<Box<dyn InitStep>> {
    if state.uses_existing_server() {
        vec![Box::new(ConnectionStep), Box::new(VersionsStep)]
    } else {
        vec![Box::new(VersionsStep)]
    }
}
