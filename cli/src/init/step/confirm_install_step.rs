use super::{InitStep, StepCompletion, StepFuture};
use crate::init::InitializationState;
use crate::init::prompt::{PromptRequest, PromptResponse};

pub(in crate::init) struct ConfirmInstallStep;
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
                    PromptRequest::confirm(include_str!("../confirm_install.txt").trim_end())
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
