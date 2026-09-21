use super::{
    database_engine_versions::PostgresVersions,
    presentation::{HomePathError, Notice},
    prompt::{PromptRequest, PromptResponse},
};

use std::path::PathBuf;

use ::sql_fun_core::InitializeArgs;
use ::statig::Outcome;

pub(super) struct Initialization {
    yes: bool,
    supplied_home: bool,
    home: PathBuf,
    default_home: String,
    version_args: Vec<String>,
    versions: Vec<PostgresVersions>,
    notices: Vec<Notice>,
    error: Option<anyhow::Error>,
}

impl From<&InitializeArgs> for Initialization {
    fn from(args: &InitializeArgs) -> Self {
        let home = args
            .sql_fun_home()
            .clone()
            .unwrap_or_else(|| PathBuf::from(shellexpand::tilde("~/.sqlfun").as_ref()));
        Self {
            yes: args.assume_yes(),
            supplied_home: args.sql_fun_home().is_some(),
            default_home: home.to_string_lossy().into_owned(),
            home,
            version_args: args.engine_versions(),
            versions: Vec::new(),
            notices: Vec::new(),
            error: None,
        }
    }
}

impl Initialization {
    pub(super) fn present(&mut self) {
        for notice in self.notices.drain(..) {
            notice.show();
        }
    }

    pub(super) fn finish(&mut self) -> Result<(), anyhow::Error> {
        match self.error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn fail(&mut self, error: anyhow::Error) -> Outcome<State> {
        self.error = Some(error);
        Outcome::Transition(State::failed())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Advance,
    Answer(PromptResponse),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Request {
    Advance,
    Prompt(PromptRequest),
}

impl Request {
    pub(super) fn run(&self) -> Result<Event, anyhow::Error> {
        match self {
            Self::Advance => Ok(Event::Advance),
            Self::Prompt(prompt) => Ok(Event::Answer(prompt.run()?)),
        }
    }
}

pub(super) struct Workflow;

#[statig::state_machine(
    initial = "State::confirm_install()",
    state(derive(Debug, PartialEq, Eq))
)]
impl Workflow {
    #[state]
    fn confirm_install(event: &Event) -> Outcome<State> {
        match event {
            Event::Answer(PromptResponse::Confirm(true)) => {
                Outcome::Transition(State::prepare_home())
            }
            Event::Answer(PromptResponse::Confirm(false)) => {
                Outcome::Transition(State::cancelled())
            }
            _ => Outcome::Handled,
        }
    }

    #[state]
    fn prepare_home(event: &Event, context: &mut Initialization) -> Outcome<State> {
        if *event != Event::Advance {
            return Outcome::Handled;
        }
        if !context.yes {
            context.notices.push(Notice::HomeDescription);
        }
        if context.supplied_home {
            if context.home.exists() {
                context
                    .notices
                    .push(Notice::HomeExists(context.home.clone()));
                Outcome::Transition(State::prepare_versions())
            } else {
                Outcome::Transition(State::confirm_create())
            }
        } else if context.yes {
            context
                .notices
                .push(Notice::HomeCreating(context.home.clone()));
            Outcome::Transition(State::create_home())
        } else {
            Outcome::Transition(State::edit_home())
        }
    }

    #[state]
    fn confirm_create(event: &Event) -> Outcome<State> {
        match event {
            Event::Answer(PromptResponse::Confirm(true)) => {
                Outcome::Transition(State::create_home())
            }
            Event::Answer(PromptResponse::Confirm(false)) => {
                Outcome::Transition(State::edit_home())
            }
            _ => Outcome::Handled,
        }
    }

    // CLI/default paths deliberately retain their original fail-fast behavior.
    #[state]
    fn create_home(event: &Event, context: &mut Initialization) -> Outcome<State> {
        if *event != Event::Advance {
            return Outcome::Handled;
        }
        if !context.supplied_home && context.home.exists() {
            context.notices.push(Notice::HomeAlreadyExists);
        } else if let Err(error) = std::fs::create_dir_all(&context.home) {
            return context.fail(error.into());
        }
        Outcome::Transition(State::prepare_versions())
    }

    #[state]
    fn edit_home(event: &Event, context: &mut Initialization) -> Outcome<State> {
        let Event::Answer(PromptResponse::Text(input)) = event else {
            return Outcome::Handled;
        };
        let path = PathBuf::from(input);
        let error = if !path.is_absolute() {
            Some(HomePathError::NotAbsolute)
        } else if path.exists() {
            None
        } else if let Some(parent) = path.parent() {
            (!parent.exists()).then(|| HomePathError::ParentNotFound(parent.to_path_buf()))
        } else {
            Some(HomePathError::ParentUnavailable)
        };
        if let Some(error) = error {
            context.notices.push(Notice::InvalidHome(error));
            return Outcome::Handled;
        }
        context.home = path;
        Outcome::Transition(State::create_edited_home())
    }

    // Only a path entered in the editor retries after a filesystem failure.
    #[state]
    fn create_edited_home(event: &Event, context: &mut Initialization) -> Outcome<State> {
        if *event != Event::Advance {
            return Outcome::Handled;
        }
        match std::fs::create_dir_all(&context.home) {
            Ok(()) => {
                context
                    .notices
                    .push(Notice::HomeCreated(context.home.clone()));
                Outcome::Transition(State::prepare_versions())
            }
            Err(error) => {
                context
                    .notices
                    .push(Notice::HomeCreateFailed(context.home.clone(), error));
                Outcome::Transition(State::edit_home())
            }
        }
    }

    #[state]
    fn prepare_versions(event: &Event, context: &mut Initialization) -> Outcome<State> {
        if *event != Event::Advance {
            return Outcome::Handled;
        }
        context.notices.push(Notice::VersionsDescription);
        match context
            .version_args
            .iter()
            .map(|version| version.parse())
            .collect()
        {
            Ok(versions) => {
                context.versions = versions;
                Outcome::Transition(State::select_versions())
            }
            Err(error) => context.fail(error),
        }
    }

    #[state]
    fn select_versions(event: &Event, context: &mut Initialization) -> Outcome<State> {
        let Event::Answer(PromptResponse::MultiSelect(indices)) = event else {
            return Outcome::Handled;
        };
        if indices.is_empty() && !context.yes {
            context.notices.push(Notice::VersionRequired);
            return Outcome::Handled;
        }
        let Some(versions) = indices
            .iter()
            .map(|&index| PostgresVersions::ALL.get(index).copied())
            .collect()
        else {
            return Outcome::Handled;
        };
        context.versions = versions;
        Outcome::Transition(State::versions_selected())
    }

    #[state]
    const fn versions_selected() -> Outcome<State> {
        Outcome::Handled
    }

    #[state]
    const fn cancelled() -> Outcome<State> {
        Outcome::Handled
    }

    #[state]
    const fn failed() -> Outcome<State> {
        Outcome::Handled
    }
}

impl State {
    pub(super) fn request(&self, context: &Initialization) -> Option<Request> {
        let prompt = match self {
            Self::ConfirmInstall {} => {
                PromptRequest::confirm(include_str!("confirm_install.txt").trim_end())
                    .with_automatic(context.yes.then_some(PromptResponse::Confirm(true)))
            }
            Self::ConfirmCreate {} => PromptRequest::confirm(
                format!(
                    include_str!("confirm_create.txt"),
                    path = context.home.display()
                )
                .trim_end(),
            )
            .with_automatic(context.yes.then_some(PromptResponse::Confirm(true))),
            Self::EditHome {} => PromptRequest::text(
                include_str!("home_prompt.txt").trim_end(),
                &context.default_home,
            ),
            Self::SelectVersions {} => {
                let options: Vec<_> = PostgresVersions::ALL
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                let defaults: Vec<_> = PostgresVersions::ALL
                    .iter()
                    .enumerate()
                    .filter_map(|(index, version)| {
                        context.versions.contains(version).then_some(index)
                    })
                    .collect();
                let automatic = context.yes.then(|| {
                    PromptResponse::MultiSelect(
                        context
                            .versions
                            .iter()
                            .filter_map(|version| {
                                PostgresVersions::ALL
                                    .iter()
                                    .position(|option| option == version)
                            })
                            .collect(),
                    )
                });
                PromptRequest::multi_select(
                    include_str!("versions_prompt.txt").trim_end(),
                    &options,
                    &defaults,
                )
                .with_automatic(automatic)
            }
            Self::PrepareHome {}
            | Self::CreateHome {}
            | Self::CreateEditedHome {}
            | Self::PrepareVersions {} => return Some(Request::Advance),
            Self::VersionsSelected {} | Self::Cancelled {} | Self::Failed {} => return None,
        };
        Some(Request::Prompt(prompt))
    }

    pub(super) const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled {})
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Event, Initialization, PostgresVersions, PromptResponse, Request, State, Workflow,
    };
    use ::clap::Parser;
    use ::statig::blocking::{IntoStateMachineExt, StateMachine};
    use std::path::Path;

    struct Scenario {
        machine: StateMachine<Workflow>,
        context: Initialization,
    }

    impl Scenario {
        fn new(args: &[&str]) -> Self {
            let args = sql_fun_core::InitializeArgs::try_parse_from(
                std::iter::once("init").chain(args.iter().copied()),
            )
            .unwrap();
            Self {
                machine: Workflow.state_machine(),
                context: Initialization::from(&args),
            }
        }

        fn with_home(path: &Path, yes: bool) -> Self {
            let mut args = vec!["--sql-fun-home", path.to_str().unwrap()];
            if yes {
                args.push("--yes");
            }
            Self::new(&args)
        }

        fn send(&mut self, event: &Event) {
            self.machine.handle_with_context(event, &mut self.context);
        }

        fn advance(&mut self) {
            self.send(&Event::Advance);
        }

        fn answer(&mut self, response: PromptResponse) {
            self.send(&Event::Answer(response));
        }

        fn state(&self) -> &State {
            self.machine.state()
        }

        fn request(&self) -> Option<Request> {
            self.state().request(&self.context)
        }

        fn accept_install(&mut self) {
            self.answer(PromptResponse::Confirm(true));
            assert_eq!(self.state(), &State::prepare_home());
            self.advance();
        }

        fn run_automatic(&mut self) {
            // Never execute a terminal prompt, even if a regression removes an automatic answer.
            for _ in 0..16 {
                let Some(request) = self.request() else {
                    return;
                };
                let event = match request {
                    Request::Advance => Event::Advance,
                    Request::Prompt(prompt) => Event::Answer(
                        prompt
                            .automatic_response()
                            .expect("non-interactive answer")
                            .clone(),
                    ),
                };
                self.send(&event);
            }
            panic!("initialization did not terminate");
        }
    }

    #[test]
    fn accepts_existing_cli_path_even_when_it_is_a_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut scenario = Scenario::with_home(file.path(), false);
        scenario.accept_install();
        assert_eq!(scenario.state(), &State::prepare_versions());
        assert_eq!(scenario.context.home, file.path());
        scenario.advance();
        assert_eq!(scenario.state(), &State::select_versions());
    }

    #[test]
    fn assumed_yes_creates_missing_parents() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("parent/home");
        let mut scenario = Scenario::with_home(&home, true);
        scenario.run_automatic();
        assert_eq!(scenario.state(), &State::versions_selected());
        assert!(home.is_dir());
    }

    #[test]
    fn assumed_yes_returns_creation_failure() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut scenario = Scenario::with_home(&file.path().join("home"), true);
        scenario.run_automatic();
        assert_eq!(scenario.state(), &State::failed());
        assert!(
            scenario
                .context
                .finish()
                .unwrap_err()
                .downcast_ref::<std::io::Error>()
                .is_some()
        );
    }

    #[test]
    fn assumed_yes_preserves_order_and_duplicates() {
        let home = tempfile::tempdir().unwrap();
        let mut scenario = Scenario::new(&[
            "--yes",
            "--sql-fun-home",
            home.path().to_str().unwrap(),
            "--engine-versions",
            "18,16,18",
        ]);
        scenario.run_automatic();
        assert_eq!(scenario.state(), &State::versions_selected());
        assert_eq!(
            scenario.context.versions,
            vec![
                PostgresVersions::Postgres18,
                PostgresVersions::Postgres16,
                PostgresVersions::Postgres18
            ]
        );
    }

    #[test]
    fn rejects_unknown_versions_before_prompting() {
        let home = tempfile::tempdir().unwrap();
        for yes in [false, true] {
            let mut scenario = Scenario::with_home(home.path(), yes);
            scenario.context.version_args = vec!["19".to_owned()];
            scenario.accept_install();
            scenario.advance();
            assert_eq!(scenario.state(), &State::failed());
            assert!(scenario.context.finish().is_err());
            assert_eq!(scenario.request(), None);
        }
    }
}
