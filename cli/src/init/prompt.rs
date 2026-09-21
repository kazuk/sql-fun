//! The terminal boundary depends only on input primitives, never workflow states.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PromptResponse {
    Confirm(bool),
    Text(String),
    MultiSelect(Vec<usize>),
}

impl PromptResponse {
    pub(super) const fn confirm(confirmed: bool) -> Self {
        Self::Confirm(confirmed)
    }

    pub(super) fn text(input: &str) -> Self {
        Self::Text(input.to_owned())
    }

    pub(super) fn multi_select(indices: &[usize]) -> Self {
        Self::MultiSelect(indices.to_vec())
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PromptRequest {
    title: String,
    input: Input,
    automatic: Option<PromptResponse>,
}

impl PromptRequest {
    pub(super) fn confirm(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            input: Input::Confirm,
            automatic: None,
        }
    }

    pub(super) fn text(title: &str, default: &str) -> Self {
        Self {
            title: title.to_owned(),
            input: Input::Text(TextInput {
                default: default.to_owned(),
                initial: String::new(),
            }),
            automatic: None,
        }
    }

    pub(super) fn with_initial_text(mut self, initial: &str) -> Self {
        if let Input::Text(input) = &mut self.input {
            initial.clone_into(&mut input.initial);
        }
        self
    }

    pub(super) fn multi_select(title: &str, options: &[String], defaults: &[usize]) -> Self {
        Self {
            title: title.to_owned(),
            input: Input::MultiSelect(Selection {
                options: options.to_vec(),
                defaults: defaults.to_vec(),
            }),
            automatic: None,
        }
    }

    pub(super) fn with_automatic(mut self, response: Option<PromptResponse>) -> Self {
        self.automatic = response;
        self
    }

    pub(super) const fn automatic_response(&self) -> Option<&PromptResponse> {
        self.automatic.as_ref()
    }

    pub(super) fn run(&self) -> Result<PromptResponse, anyhow::Error> {
        if let Some(response) = self.automatic_response() {
            return Ok(response.clone());
        }
        match &self.input {
            Input::Confirm => Ok(PromptResponse::confirm(
                inquire::Confirm::new(&self.title).prompt()?,
            )),
            Input::Text(input) => Ok(PromptResponse::text(
                &inquire::Text::new(&self.title)
                    .with_default(&input.default)
                    .with_initial_value(&input.initial)
                    .prompt()?,
            )),
            Input::MultiSelect(selection) => Ok(PromptResponse::multi_select(
                &inquire::MultiSelect::new(&self.title, selection.options.clone())
                    .with_default(&selection.defaults)
                    .raw_prompt()?
                    .into_iter()
                    .map(|item| item.index)
                    .collect::<Vec<_>>(),
            )),
        }
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
