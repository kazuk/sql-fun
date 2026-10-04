use super::{Event, Initialization, PostgresVersions, Request};
use std::path::PathBuf;

pub fn initialization(yes: bool) -> Initialization {
    Initialization {
        yes,
        server_source: false,
        supplied_home: true,
        home: PathBuf::from("chosen/home"),
        default_home: "original/home".to_owned(),
        home_input: String::new(),
        version_args: Vec::new(),
        versions: PostgresVersions::all(),
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
