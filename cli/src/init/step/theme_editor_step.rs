use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepFuture},
};

pub(in crate::init) struct ThemeEditorStep;
impl InitStep for ThemeEditorStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            let automatic = state
                .theme
                .as_mut()
                .expect("theme operation")
                .choose_editor(state.input.as_mut())?;
            Ok(direction.completed(automatic))
        })
    }
}
