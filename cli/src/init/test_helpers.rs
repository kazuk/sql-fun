use super::{
    InitializationState,
    error::InitError,
    prompt::{PromptRequest, PromptResponse},
    step::PromptInput,
};
use ::clap::Parser;
use ::sql_fun_core::{InitializeArgs, SqlFunArgs};
use std::{
    cell::RefCell,
    collections::VecDeque,
    path::{Path, PathBuf},
    rc::Rc,
};

pub(super) fn common_args() -> SqlFunArgs {
    SqlFunArgs::try_parse_from(["sql-fun"]).unwrap()
}

pub(super) fn state<'a>(common: &'a SqlFunArgs, options: &[&str]) -> InitializationState<'a> {
    let args =
        InitializeArgs::try_parse_from(std::iter::once("init").chain(options.iter().copied()))
            .unwrap();
    InitializationState::new(common, &args, false)
}

pub(super) struct Answer {
    title: String,
    response: Result<PromptResponse, InitError>,
    home: Option<(PathBuf, bool)>,
    expected_request: Option<PromptRequest>,
}
impl Answer {
    pub(super) fn confirm(title: &str, value: bool) -> Self {
        Self::response(title, PromptResponse::confirm(value))
    }
    pub(super) fn text(title: &str, value: &str) -> Self {
        Self::response(title, PromptResponse::text(value))
    }
    pub(super) fn choose(title: &str, values: &[usize]) -> Self {
        Self::response(title, PromptResponse::multi_select(values))
    }
    fn response(title: &str, response: PromptResponse) -> Self {
        Self {
            title: title.to_owned(),
            response: Ok(response),
            home: None,
            expected_request: None,
        }
    }
    pub(super) fn interrupted(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            response: InitError::prompt(inquire::InquireError::OperationInterrupted),
            home: None,
            expected_request: None,
        }
    }
    pub(super) fn cancelled(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            response: InitError::prompt(inquire::InquireError::OperationCanceled),
            home: None,
            expected_request: None,
        }
    }
    pub(super) fn with_home(mut self, home: &Path, exists: bool) -> Self {
        self.home = Some((home.to_path_buf(), exists));
        self
    }
    pub(super) fn expecting(mut self, request: PromptRequest) -> Self {
        self.expected_request = Some(request);
        self
    }
}

#[derive(Clone)]
pub(super) struct Script(Rc<RefCell<VecDeque<Answer>>>);
impl Script {
    pub(super) fn install(state: &mut InitializationState<'_>, answers: Vec<Answer>) -> Self {
        let script = Self(Rc::new(RefCell::new(answers.into())));
        state.set_input(script.clone());
        script
    }
    pub(super) fn assert_finished(&self) {
        assert!(self.0.borrow().is_empty(), "unconsumed scripted answers");
    }
}
impl PromptInput for Script {
    fn answer(&mut self, request: &PromptRequest) -> Result<PromptResponse, InitError> {
        if let Some(answer) = request.automatic_response() {
            return Ok(answer.clone());
        }
        let answer = self.0.borrow_mut().pop_front().expect("unexpected prompt");
        assert!(
            request.title().contains(&answer.title),
            "expected {}, got {}",
            answer.title,
            request.title()
        );
        if let Some((home, exists)) = &answer.home {
            assert_eq!(home.exists(), *exists, "home timing at {}", request.title());
        }
        if let Some(expected) = answer.expected_request {
            assert_eq!(request, &expected);
        }
        answer.response
    }
}

/// A task-owned home and existing user content for initialization scenarios.
pub(super) struct HomeFixture(::tempfile::TempDir);
impl HomeFixture {
    pub(super) fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    pub(super) fn home(&self) -> PathBuf {
        self.0.path().join("home")
    }
    pub(super) fn existing_home(&self) -> &Path {
        self.0.path()
    }
    pub(super) fn blocked_home(&self) -> PathBuf {
        let path = self.0.path().join("blocked");
        std::fs::write(&path, "existing content").unwrap();
        path
    }
    pub(super) fn missing_parent_home(&self) -> PathBuf {
        self.0.path().join("missing/home")
    }
    pub(super) fn assert_preserved(&self) {
        assert_eq!(
            std::fs::read_to_string(self.0.path().join("blocked")).unwrap(),
            "existing content"
        );
    }
}

pub(super) struct MetadataProject {
    directory: ::tempfile::TempDir,
    common: SqlFunArgs,
}
impl MetadataProject {
    pub(super) fn new(empty: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("Cargo.toml");
        let text = if empty {
            "[package.metadata.database]\nconnector = 'example'\n"
        } else {
            "[package.metadata.database.local]\nengine = 'sqlite'\n[package.metadata.database.pg]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '17'\n"
        };
        std::fs::write(&manifest, text).unwrap();
        let common =
            SqlFunArgs::try_parse_from(["sql-fun", "--metadata-file", manifest.to_str().unwrap()])
                .unwrap();
        Self { directory, common }
    }
    pub(super) fn state(&self) -> InitializationState<'_> {
        let mut state = state(&self.common, &[]);
        state.set_metadata_requested(true);
        state.home.set_home(&self.directory.path().join("home"));
        state
    }
}
