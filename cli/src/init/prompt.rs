//! The terminal boundary depends only on input primitives, never workflow states.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PromptResponse {
    Confirm(bool),
    Text(String),
    MultiSelect(Vec<usize>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Input {
    Confirm,
    Text(String),
    MultiSelect(Selection),
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
            input: Input::Text(default.to_owned()),
            automatic: None,
        }
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
            Input::Confirm => Ok(PromptResponse::Confirm(
                inquire::Confirm::new(&self.title).prompt()?,
            )),
            Input::Text(default) => Ok(PromptResponse::Text(
                inquire::Text::new(&self.title)
                    .with_default(default)
                    .prompt()?,
            )),
            Input::MultiSelect(selection) => Ok(PromptResponse::MultiSelect(
                inquire::MultiSelect::new(&self.title, selection.options.clone())
                    .with_default(&selection.defaults)
                    .raw_prompt()?
                    .into_iter()
                    .map(|item| item.index)
                    .collect(),
            )),
        }
    }
}
