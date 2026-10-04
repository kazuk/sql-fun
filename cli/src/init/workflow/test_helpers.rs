use super::{Event, Initialization, Request};
use std::path::PathBuf;

pub fn initialization(yes: bool) -> Initialization {
    Initialization {
        yes,
        supplied_home: true,
        home: PathBuf::from("chosen/home"),
        default_home: "original/home".to_owned(),
        home_input: String::new(),
        notices: Vec::new(),
        error: None,
    }
}

impl Request {
    /// Resolve test inputs without ever invoking a terminal prompt.
    pub fn automatic_event(&self) -> Event {
        match self {
            Self::Advance => Event::Advance,
            Self::Prompt(prompt) => Event::Answer(
                prompt
                    .automatic_response()
                    .expect("non-interactive answer")
                    .clone(),
            ),
        }
    }
}
