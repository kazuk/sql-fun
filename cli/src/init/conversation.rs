use crate::init::{
    InitializationState,
    error::InitError,
    step::{Direction, InitStep, StepCompletion},
};

/// Traverses the configured vector and returns navigation past either edge to
/// the containing conversation. Nested entry direction is always explicit.
pub(super) struct Conversation(Vec<Box<dyn InitStep>>);

impl Conversation {
    pub(super) fn new(steps: Vec<Box<dyn InitStep>>) -> Self {
        Self(steps)
    }

    pub(super) async fn run(
        &mut self,
        state: &mut InitializationState<'_>,
        mut direction: Direction,
    ) -> Result<StepCompletion, anyhow::Error> {
        let mut cursor = if direction.is_backward() {
            self.0.len().saturating_sub(1)
        } else {
            0
        };
        while cursor < self.0.len() {
            let completion = match self.0[cursor].prompt(state, direction).await {
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
                direction = Direction::backward();
                if cursor == 0 {
                    return Ok(StepCompletion::back());
                }
                cursor -= 1;
            } else {
                direction = Direction::forward();
                cursor += 1;
            }
        }
        Ok(direction.completion())
    }
}

#[cfg(test)]
mod tests;
