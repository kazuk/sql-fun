use super::{
    InitializationState,
    error::InitError,
    step::{InitStep, StepCompletion},
};

/// Traverses only the configured vector. Nested engines share traversal direction
/// and input activity, so re-entry reaches the preceding visible prompt without
/// knowing which steps contain questions or child conversations.
pub(super) struct Conversation(Vec<Box<dyn InitStep>>);

impl Conversation {
    pub(super) fn new(steps: Vec<Box<dyn InitStep>>) -> Self {
        Self(steps)
    }

    pub(super) async fn run(
        &mut self,
        state: &mut InitializationState<'_>,
    ) -> Result<StepCompletion, anyhow::Error> {
        let mut cursor = if state.backward {
            self.0.len().saturating_sub(1)
        } else {
            0
        };
        while cursor < self.0.len() {
            let prompts = state.input.prompts();
            let completion = match self.0[cursor].prompt(state).await {
                Ok(completion) => completion,
                Err(error) => {
                    if let Some(prompt_error) = error.downcast_ref::<InitError>()
                        && let Some(completion) = prompt_error.navigation()
                    {
                        completion
                    } else {
                        return Err(error);
                    }
                }
            };
            if completion.is_cancelled() {
                return Ok(completion);
            }
            if completion.is_back() {
                state.backward = true;
            } else if state.input.prompts() != prompts {
                state.backward = false;
            }
            if state.backward {
                if cursor == 0 {
                    return Ok(StepCompletion::back());
                }
                cursor -= 1;
            } else {
                cursor += 1;
            }
        }
        Ok(StepCompletion::next())
    }
}

#[cfg(test)]
mod tests;
