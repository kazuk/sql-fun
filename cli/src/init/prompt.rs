//! The terminal boundary depends only on input primitives, never workflow states.

use super::error::InitError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptResponse {
    Confirm(bool),
    Text(String),
    MultiSelect(Vec<usize>),
}

impl PromptResponse {
    pub const fn confirm(confirmed: bool) -> Self {
        Self::Confirm(confirmed)
    }

    pub fn text(input: &str) -> Self {
        Self::Text(input.to_owned())
    }

    pub fn multi_select(indices: &[usize]) -> Self {
        Self::MultiSelect(indices.to_vec())
    }
}

impl PromptResponse {
    pub const fn confirmation(&self) -> Option<bool> {
        if let Self::Confirm(confirmed) = self {
            Some(*confirmed)
        } else {
            None
        }
    }

    pub fn text_input(&self) -> Option<&str> {
        if let Self::Text(input) = self {
            Some(input)
        } else {
            None
        }
    }

    pub fn selected_indices(&self) -> Option<&[usize]> {
        if let Self::MultiSelect(indices) = self {
            Some(indices)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_prompt_response_values {
    use super::PromptResponse;

    #[rstest::rstest]
    #[case::confirmed(&PromptResponse::confirm(true), Some(true), None, None)]
    #[case::declined(&PromptResponse::confirm(false), Some(false), None, None)]
    #[case::text(&PromptResponse::text("home"), None, Some("home"), None)]
    #[case::empty_text(&PromptResponse::text(""), None, Some(""), None)]
    #[case::selection(&PromptResponse::multi_select(&[2, 0, 2]), None, None, Some(&[2, 0, 2][..]))]
    #[case::empty_selection(&PromptResponse::multi_select(&[]), None, None, Some(&[][..]))]
    fn exposes_only_the_matching_value(
        #[case] response: &PromptResponse,
        #[case] confirmation: Option<bool>,
        #[case] text: Option<&str>,
        #[case] indices: Option<&[usize]>,
    ) {
        assert_eq!(response.confirmation(), confirmation);
        assert_eq!(response.text_input(), text);
        assert_eq!(response.selected_indices(), indices);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Input {
    Confirm,
    Text(TextInput),
    MultiSelect(Selection),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextInput {
    default: String,
    initial: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Selection {
    options: Vec<String>,
    defaults: Vec<usize>,
}

impl Input {
    const fn confirm() -> Self {
        Self::Confirm
    }

    fn text(default: &str) -> Self {
        Self::Text(TextInput {
            default: default.to_owned(),
            initial: String::new(),
        })
    }

    fn multi_select(options: &[String], defaults: &[usize]) -> Self {
        Self::MultiSelect(Selection {
            options: options.to_vec(),
            defaults: defaults.to_vec(),
        })
    }

    fn set_initial_text(&mut self, initial: &str) {
        if let Self::Text(input) = self {
            initial.clone_into(&mut input.initial);
        }
    }

    fn run(&self, title: &str) -> Result<PromptResponse, InitError> {
        match self {
            Self::Confirm => Self::run_confirm(title),
            Self::Text(input) => Self::run_text(title, input),
            Self::MultiSelect(selection) => Self::run_multi_select(title, selection),
        }
    }

    fn run_confirm(title: &str) -> Result<PromptResponse, InitError> {
        Ok(PromptResponse::confirm(
            inquire::Confirm::new(title)
                .prompt()
                .map_err(InitError::prompt)?,
        ))
    }

    fn run_text(title: &str, input: &TextInput) -> Result<PromptResponse, InitError> {
        Ok(PromptResponse::text(
            &inquire::Text::new(title)
                .with_default(&input.default)
                .with_initial_value(&input.initial)
                .prompt()
                .map_err(InitError::prompt)?,
        ))
    }

    fn run_multi_select(title: &str, selection: &Selection) -> Result<PromptResponse, InitError> {
        Ok(PromptResponse::multi_select(
            &inquire::MultiSelect::new(title, selection.options.clone())
                .with_default(&selection.defaults)
                .raw_prompt()
                .map_err(InitError::prompt)?
                .into_iter()
                .map(|item| item.index)
                .collect::<Vec<_>>(),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptRequest {
    title: String,
    input: Input,
    automatic: Option<PromptResponse>,
}

impl PromptRequest {
    pub fn confirm(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            input: Input::confirm(),
            automatic: None,
        }
    }

    pub fn text(title: &str, default: &str) -> Self {
        Self {
            title: title.to_owned(),
            input: Input::text(default),
            automatic: None,
        }
    }

    pub fn with_initial_text(mut self, initial: &str) -> Self {
        self.input.set_initial_text(initial);
        self
    }

    pub fn multi_select(title: &str, options: &[String], defaults: &[usize]) -> Self {
        Self {
            title: title.to_owned(),
            input: Input::multi_select(options, defaults),
            automatic: None,
        }
    }

    pub fn with_automatic(mut self, response: Option<PromptResponse>) -> Self {
        self.automatic = response;
        self
    }

    pub const fn automatic_response(&self) -> Option<&PromptResponse> {
        self.automatic.as_ref()
    }

    pub fn run(&self) -> Result<PromptResponse, InitError> {
        if let Some(response) = self.automatic_response() {
            return Ok(response.clone());
        }
        self.input.run(&self.title)
    }
}

#[cfg(test)]
mod tests {
    use super::{PromptRequest, PromptResponse};

    #[rstest::rstest]
    #[case::confirm(PromptRequest::confirm("confirm"), PromptResponse::confirm(true))]
    #[case::text(
        PromptRequest::text("text", "default"),
        PromptResponse::text("supplied")
    )]
    #[case::multi_select(
        PromptRequest::multi_select("select", &["first".to_owned(), "second".to_owned()], &[0]),
        PromptResponse::multi_select(&[1]),
    )]
    fn supplied_responses_bypass_the_terminal(
        #[case] request: PromptRequest,
        #[case] response: PromptResponse,
    ) {
        assert_eq!(
            request
                .with_automatic(Some(response.clone()))
                .run()
                .unwrap(),
            response,
        );
    }
}
