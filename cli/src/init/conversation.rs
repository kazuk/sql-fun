use super::{
    InitializationState,
    step::{InitStep, StepCompletion},
};

/// Owns traversal only. Steps, including nested conversations, share the caller's
/// initialization state. The configured order establishes data availability.
pub(super) struct Conversation(Vec<Box<dyn InitStep>>);

impl Conversation {
    pub(super) fn new(steps: Vec<Box<dyn InitStep>>) -> Self {
        Self(steps)
    }

    pub(super) async fn run(
        &mut self,
        state: &mut InitializationState<'_>,
    ) -> Result<StepCompletion, anyhow::Error> {
        for step in &mut self.0 {
            let completion = step.prompt(state).await?;
            if completion.is_cancelled() {
                return Ok(completion);
            }
        }
        Ok(StepCompletion::next())
    }
}

#[cfg(test)]
mod tests;
