//! Initialization step interfaces and independently implemented step modules.
//!
//! Implement each concrete step in its own submodule and update shared state
//! through its owning type's methods. Register steps and define every nested
//! step list in `flow`; that module documents the addition and removal procedure.

mod confirm_home_step;
mod confirm_install_step;
mod connection_step;
mod datasource_selection_step;
mod edit_home_step;
mod engine_selection_step;
mod engine_steps;
mod home_directory_step;
mod home_step;
mod postgres_source_step;
mod postgres_steps;
mod selection_step;
mod source_details;
mod sqlite_step;
mod theme_editor_step;
mod theme_step;
mod versions_step;

pub(super) use self::confirm_home_step::ConfirmHomeStep;
pub(super) use self::confirm_install_step::ConfirmInstallStep;
pub(super) use self::connection_step::ConnectionStep;
pub(super) use self::datasource_selection_step::DatasourceSelectionStep;
pub(super) use self::edit_home_step::EditHomeStep;
pub(super) use self::engine_selection_step::EngineSelectionStep;
pub(super) use self::engine_steps::EngineSteps;
pub(super) use self::home_directory_step::HomeDirectoryStep;
pub(super) use self::home_step::HomeStep;
pub(super) use self::postgres_source_step::PostgresSourceStep;
pub(super) use self::postgres_steps::PostgresSteps;
pub(super) use self::selection_step::SelectionStep;
pub(super) use self::source_details::SourceDetails;
pub(super) use self::sqlite_step::SqliteStep;
pub(super) use self::theme_editor_step::ThemeEditorStep;
pub(super) use self::theme_step::ThemeStep;
pub(super) use self::versions_step::VersionsStep;

use super::{
    InitializationState,
    error::InitError,
    prompt::{PromptRequest, PromptResponse},
};
use std::{future::Future, pin::Pin};

pub(super) type StepFuture<'a> =
    Pin<Box<dyn Future<Output = Result<StepCompletion, anyhow::Error>> + 'a>>;

/// A step changes the shared state only through accepted inputs. Ordering is
/// owned by the code assembling the conversation, never by the step or engine.
pub(super) trait InitStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a>;
}

/// The edge at which a conversation is entered, also passed to automatic steps.
#[derive(Clone, Copy, Debug)]
pub(super) enum Direction {
    Forward,
    Backward,
}
impl Direction {
    pub(super) const fn forward() -> Self {
        Self::Forward
    }
    pub(super) const fn backward() -> Self {
        Self::Backward
    }
    pub(super) const fn is_backward(self) -> bool {
        matches!(self, Self::Backward)
    }
    pub(super) const fn completion(self) -> StepCompletion {
        match self {
            Self::Forward => StepCompletion::next(),
            Self::Backward => StepCompletion::back(),
        }
    }
    /// Accepted interactive answers advance; automatic steps preserve entry direction.
    pub(super) const fn completed(self, automatic: bool) -> StepCompletion {
        if automatic {
            self.completion()
        } else {
            StepCompletion::next()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StepCompletion {
    Next,
    Cancelled,
    Back,
}

impl StepCompletion {
    pub(super) const fn back() -> Self {
        Self::Back
    }
    pub(super) const fn is_back(self) -> bool {
        matches!(self, Self::Back)
    }
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

#[cfg(test)]
mod tests;
