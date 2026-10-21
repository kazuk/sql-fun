//! Input primitives and control outcomes, independent of initialization steps.

use super::error::InitError;

const NAVIGATION: &str = "Esc: Back. Ctrl+C: Cancel.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptResponse {
    Confirm(bool),
    Text(String),
    Selection(Vec<usize>),
}
impl PromptResponse {
    pub const fn confirm(value: bool) -> Self {
        Self::Confirm(value)
    }
    pub fn text(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
    pub fn selection(value: &[usize]) -> Self {
        Self::Selection(value.to_vec())
    }
    pub const fn confirmation(&self) -> Option<bool> {
        if let Self::Confirm(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    pub fn text_input(&self) -> Option<&str> {
        if let Self::Text(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn indices(&self) -> &[usize] {
        if let Self::Selection(value) = self {
            value
        } else {
            &[]
        }
    }
    pub fn selected_names(&self, options: &[String]) -> Vec<String> {
        match self {
            Self::Selection(indices) => indices
                .iter()
                .filter_map(|index| options.get(*index).cloned())
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PromptOutcome {
    Answer(PromptResponse),
    Back,
    Cancel,
}
impl PromptOutcome {
    pub fn answer(value: &PromptResponse) -> Self {
        Self::Answer(value.clone())
    }
    pub const fn back() -> Self {
        Self::Back
    }
    pub const fn cancel() -> Self {
        Self::Cancel
    }
    pub const fn response(&self) -> Option<&PromptResponse> {
        if let Self::Answer(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub const fn is_back(&self) -> bool {
        matches!(self, Self::Back)
    }
    pub const fn is_cancel(&self) -> bool {
        matches!(self, Self::Cancel)
    }
    fn from_result(
        result: Result<PromptResponse, inquire::InquireError>,
    ) -> Result<Self, InitError> {
        match result {
            Ok(value) => Ok(Self::answer(&value)),
            Err(inquire::InquireError::OperationCanceled) => Ok(Self::back()),
            Err(inquire::InquireError::OperationInterrupted) => Ok(Self::cancel()),
            Err(error) => InitError::prompt(error),
        }
    }
}

#[cfg(test)]
mod test_outcomes {
    use super::PromptOutcome;
    #[test]
    fn only_control_errors_become_navigation() {
        assert!(
            PromptOutcome::from_result(Err(inquire::InquireError::OperationCanceled))
                .unwrap()
                .is_back()
        );
        assert!(
            PromptOutcome::from_result(Err(inquire::InquireError::OperationInterrupted))
                .unwrap()
                .is_cancel()
        );
        assert!(PromptOutcome::from_result(Err(inquire::InquireError::NotTTY)).is_err());
    }
}

pub enum PromptRequest {
    Confirm(String),
    Text(String),
    Select(Selection),
    MultiSelect(Selection),
}

pub struct Selection {
    title: String,
    options: Vec<String>,
    exactly_one: bool,
}
impl Selection {
    fn new(title: &str, options: &[String], exactly_one: bool) -> Self {
        Self {
            title: title.to_owned(),
            options: options.to_vec(),
            exactly_one,
        }
    }
    fn validate<T>(exactly_one: bool, selected: &[T]) -> inquire::validator::Validation {
        if exactly_one && selected.len() != 1 {
            inquire::validator::Validation::Invalid("Select exactly one action explicitly.".into())
        } else if selected.is_empty() {
            inquire::validator::Validation::Invalid("Select at least one option explicitly.".into())
        } else {
            inquire::validator::Validation::Valid
        }
    }
}
impl PromptRequest {
    pub fn confirm(title: &str) -> Self {
        Self::Confirm(title.to_owned())
    }
    pub fn text(title: &str) -> Self {
        Self::Text(title.to_owned())
    }
    pub fn select(title: &str, options: &[String]) -> Self {
        Self::Select(Selection::new(title, options, true))
    }
    pub fn multi_select(title: &str, options: &[String]) -> Self {
        Self::MultiSelect(Selection::new(title, options, false))
    }
    pub fn authorization() -> Self {
        Self::MultiSelect(Selection::new(
            "Choose one action to finish initialization",
            &["Ok".to_owned(), "Cancel".to_owned()],
            true,
        ))
    }
    pub fn run(&self, previous: Option<&PromptResponse>) -> Result<PromptOutcome, InitError> {
        PromptOutcome::from_result(self.read(previous))
    }
    fn read(
        &self,
        previous: Option<&PromptResponse>,
    ) -> Result<PromptResponse, inquire::InquireError> {
        let render = inquire::ui::RenderConfig::default()
            .with_canceled_prompt_indicator(inquire::ui::Styled::new("<back>"));
        match self {
            Self::Confirm(title) => {
                let help = format!("Type y/n, Enter to confirm. {NAVIGATION}");
                let mut prompt = inquire::Confirm::new(title)
                    .with_render_config(render)
                    .with_help_message(&help);
                if let Some(answer) = previous
                    && let Some(value) = answer.confirmation()
                {
                    prompt = prompt.with_default(value);
                }
                prompt.prompt().map(PromptResponse::confirm)
            }
            Self::Text(title) => {
                let help = format!("Enter to confirm. {NAVIGATION}");
                let mut prompt = inquire::Text::new(title)
                    .with_render_config(render)
                    .with_help_message(&help);
                if let Some(answer) = previous
                    && let Some(value) = answer.text_input()
                {
                    prompt = prompt.with_initial_value(value);
                }
                prompt.prompt().map(|value| PromptResponse::text(&value))
            }
            Self::Select(selection) => {
                inquire::Select::new(&selection.title, selection.options.clone())
                    .with_render_config(render)
                    .with_starting_cursor(if let Some(answer) = previous {
                        answer.indices().first().copied().unwrap_or(0)
                    } else {
                        0
                    })
                    .with_help_message(&format!(
                        "Use arrows to move, Enter to choose one. {NAVIGATION}"
                    ))
                    .raw_prompt()
                    .map(|value| PromptResponse::selection(&[value.index]))
            }
            Self::MultiSelect(selection) => {
                let exactly_one = selection.exactly_one;
                inquire::MultiSelect::new(&selection.title, selection.options.clone())
                    .with_render_config(render)
                    .with_default(if let Some(answer) = previous {
                        answer.indices()
                    } else {
                        &[]
                    })
                    .with_help_message(&format!(
                        "Use arrows to move, Space to select, Enter to confirm. {NAVIGATION}"
                    ))
                    .with_validator(
                        move |values: &[inquire::list_option::ListOption<&String>]| {
                            Ok(Selection::validate(exactly_one, values))
                        },
                    )
                    .raw_prompt()
                    .map(|values| {
                        PromptResponse::selection(
                            &values
                                .into_iter()
                                .map(|value| value.index)
                                .collect::<Vec<_>>(),
                        )
                    })
            }
        }
    }
}
