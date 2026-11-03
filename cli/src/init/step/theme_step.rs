use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepFuture},
    theme::ThemePlan,
};

/// Inspect only the confirmed home's mandatory theme and collect its operation.
pub(in crate::init) struct ThemeStep;
impl InitStep for ThemeStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            let mut plan = ThemePlan::inspect(state.home.home())?;
            let automatic = plan.choose(&state.args, state.theme.as_ref(), state.input.as_mut())?;
            state.theme = Some(plan);
            Ok(direction.completed(automatic))
        })
    }
}
