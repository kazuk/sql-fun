use super::{Event, Initialization, PostgresVersions, PromptResponse, Request, State, Workflow};
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
        self.answer(PromptResponse::confirm(true));
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

#[test]
fn initial_state_requests_install_confirmation() {
    let scenario = Scenario::new(&[]);
    assert_eq!(scenario.state(), &State::confirm_install());
    assert_eq!(
        scenario.request(),
        Some(Request::Prompt(super::PromptRequest::confirm(
            include_str!("../confirm_install.txt").trim_end()
        ),))
    );
}

#[test]
fn rejecting_install_cancels_without_creating_home() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let mut scenario = Scenario::with_home(&home, false);
    scenario.answer(PromptResponse::confirm(false));
    assert!(scenario.state().is_cancelled());
    assert_eq!(scenario.request(), None);
    assert!(!home.exists());
    assert!(scenario.context.finish().is_ok());
}

#[test]
fn missing_cli_home_requires_confirmation_then_creates_parents() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("parent/home");
    let mut scenario = Scenario::with_home(&home, false);
    scenario.accept_install();
    assert_eq!(scenario.state(), &State::confirm_create());
    assert_eq!(
        scenario.request(),
        Some(Request::Prompt(super::PromptRequest::confirm(
            format!(include_str!("../confirm_create.txt"), path = home.display()).trim_end()
        ),))
    );
    assert!(!home.exists());
    scenario.answer(PromptResponse::confirm(true));
    assert_eq!(scenario.state(), &State::create_home());
    assert_eq!(scenario.request(), Some(Request::Advance));
    scenario.advance();
    assert_eq!(scenario.state(), &State::prepare_versions());
    assert!(home.is_dir());
}

#[test]
fn confirmed_cli_path_creation_failure_terminates_without_editing() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut scenario = Scenario::with_home(&file.path().join("home"), false);
    scenario.accept_install();
    scenario.answer(PromptResponse::confirm(true));
    scenario.advance();
    assert_eq!(scenario.state(), &State::failed());
    assert_eq!(scenario.request(), None);
    assert!(scenario.context.finish().is_err());
}

#[test]
fn declining_creation_requests_editor_with_original_path() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let mut scenario = Scenario::with_home(&home, false);
    scenario.accept_install();
    scenario.answer(PromptResponse::confirm(false));
    assert_eq!(scenario.state(), &State::edit_home());
    assert_eq!(
        scenario.request(),
        Some(Request::Prompt(super::PromptRequest::text(
            include_str!("../home_prompt.txt").trim_end(),
            home.to_str().unwrap()
        ),))
    );
    assert!(!home.exists());
}

#[test]
fn default_home_still_uses_sqlfun_and_opens_editor_interactively() {
    let mut scenario = Scenario::new(&[]);
    assert_eq!(
        scenario.context.home,
        std::path::PathBuf::from(shellexpand::tilde("~/.sqlfun").as_ref())
    );
    scenario.accept_install();
    assert_eq!(scenario.state(), &State::edit_home());
    assert!(matches!(scenario.request(), Some(Request::Prompt(_))));
}

#[test]
fn edited_home_creation_failure_retries_then_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let original_home = root.path().join("original");
    let blocked_home = tempfile::NamedTempFile::new().unwrap();
    let new_home = root.path().join("new");
    let mut scenario = Scenario::with_home(&original_home, false);
    scenario.accept_install();
    scenario.answer(PromptResponse::confirm(false));
    let editor_request = scenario.request();
    // An existing file passes the original path validator but cannot become a directory.
    scenario.answer(PromptResponse::text(blocked_home.path().to_str().unwrap()));
    assert_eq!(scenario.state(), &State::create_edited_home());
    scenario.advance();
    assert_eq!(scenario.state(), &State::edit_home());
    assert_eq!(scenario.request(), editor_request);
    assert!(scenario.context.error.is_none());
    assert!(matches!(
        scenario.context.notices.last(),
        Some(super::Notice::HomeCreateFailed(_, _))
    ));
    scenario.answer(PromptResponse::text(new_home.to_str().unwrap()));
    scenario.advance();
    assert_eq!(scenario.state(), &State::prepare_versions());
    assert!(new_home.is_dir());
    assert!(!original_home.exists());
    assert_eq!(scenario.context.home, new_home);
}

#[test]
fn editor_rejects_relative_paths_and_missing_parents() {
    let root = tempfile::tempdir().unwrap();
    let mut scenario = Scenario::new(&[]);
    scenario.accept_install();
    scenario.answer(PromptResponse::text("relative/home"));
    assert_eq!(scenario.state(), &State::edit_home());
    assert!(matches!(
        scenario.context.notices.last(),
        Some(super::Notice::InvalidHome(
            super::HomePathError::NotAbsolute
        ))
    ));
    let missing = root.path().join("missing/home");
    scenario.answer(PromptResponse::text(missing.to_str().unwrap()));
    assert_eq!(scenario.state(), &State::edit_home());
    assert!(
        matches!(scenario.context.notices.last(), Some(super::Notice::InvalidHome(super::HomePathError::ParentNotFound(parent))) if parent == missing.parent().unwrap())
    );
    assert!(!missing.exists());
    assert_eq!(
        scenario.request(),
        Some(Request::Prompt(
            super::PromptRequest::text(
                include_str!("../home_prompt.txt").trim_end(),
                &scenario.context.default_home,
            )
            .with_initial_text(missing.to_str().unwrap())
        ))
    );
}

#[test]
fn editor_accepts_existing_directory() {
    let root = tempfile::tempdir().unwrap();
    let mut scenario = Scenario::new(&[]);
    scenario.accept_install();
    scenario.answer(PromptResponse::text(root.path().to_str().unwrap()));
    scenario.advance();
    assert_eq!(scenario.state(), &State::prepare_versions());
    assert_eq!(scenario.context.home, root.path());
}

#[test]
fn cli_still_accepts_relative_paths() {
    let root = tempfile::tempdir_in(".").unwrap();
    let relative = root
        .path()
        .strip_prefix(std::env::current_dir().unwrap())
        .unwrap();
    assert!(!relative.is_absolute());
    let mut scenario = Scenario::with_home(relative, true);
    scenario.run_automatic();
    assert_eq!(scenario.state(), &State::versions_selected());
    assert_eq!(scenario.context.home, relative);
}

#[test]
fn assumed_yes_default_home_creates_or_reuses_directory() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    for already_exists in [false, true] {
        let mut scenario = Scenario::new(&["--yes"]);
        // Redirect the resolved default into a temporary directory without changing HOME.
        scenario.context.home.clone_from(&home);
        scenario.run_automatic();
        assert_eq!(scenario.state(), &State::versions_selected());
        assert!(home.is_dir());
        assert_eq!(
            scenario
                .context
                .notices
                .iter()
                .any(|notice| matches!(notice, super::Notice::HomeAlreadyExists)),
            already_exists
        );
    }
}

#[test]
fn assumed_yes_default_home_creation_failure_terminates() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut scenario = Scenario::new(&["--yes"]);
    scenario.context.home = file.path().join("home");
    scenario.run_automatic();
    assert_eq!(scenario.state(), &State::failed());
    assert_eq!(scenario.request(), None);
    assert!(scenario.context.finish().is_err());
}

#[test]
fn engine_arguments_set_initial_selection_but_interactive_answer_decides() {
    let root = tempfile::tempdir().unwrap();
    let mut scenario = Scenario::new(&[
        "--sql-fun-home",
        root.path().to_str().unwrap(),
        "--engine-versions",
        "18,16,18",
    ]);
    scenario.accept_install();
    scenario.advance();
    assert_eq!(scenario.state(), &State::select_versions());
    assert_eq!(
        scenario.request(),
        Some(Request::Prompt(super::PromptRequest::multi_select(
            include_str!("../versions_prompt.txt").trim_end(),
            &[
                "PostgreSQL Version 16".to_owned(),
                "PostgreSQL Version 17".to_owned(),
                "PostgreSQL Version 18".to_owned()
            ],
            &[0, 2],
        )))
    );
    scenario.answer(PromptResponse::multi_select(&[1]));
    assert_eq!(scenario.state(), &State::versions_selected());
    assert_eq!(
        scenario.context.versions,
        vec![PostgresVersions::Postgres17]
    );
    assert_eq!(scenario.request(), None);
    assert!(scenario.context.finish().is_ok());
}

#[test]
fn empty_interactive_engine_selection_retries() {
    let root = tempfile::tempdir().unwrap();
    let mut scenario = Scenario::with_home(root.path(), false);
    scenario.accept_install();
    scenario.advance();
    scenario.answer(PromptResponse::multi_select(&[]));
    assert_eq!(scenario.state(), &State::select_versions());
    assert_eq!(
        scenario.request(),
        Some(Request::Prompt(super::PromptRequest::multi_select(
            include_str!("../versions_prompt.txt").trim_end(),
            &[
                "PostgreSQL Version 16".to_owned(),
                "PostgreSQL Version 17".to_owned(),
                "PostgreSQL Version 18".to_owned()
            ],
            &[],
        )))
    );
    assert!(matches!(
        scenario.context.notices.last(),
        Some(super::Notice::VersionRequired)
    ));
}

#[test]
fn defaults_select_all_supported_versions_non_interactively() {
    let root = tempfile::tempdir().unwrap();
    let mut scenario = Scenario::with_home(root.path(), true);
    scenario.run_automatic();
    assert_eq!(scenario.state(), &State::versions_selected());
    assert_eq!(scenario.context.versions, PostgresVersions::ALL);
}

#[test]
fn unrelated_answer_does_not_advance_or_touch_home() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let mut scenario = Scenario::with_home(&home, false);
    let request = scenario.request();
    scenario.answer(PromptResponse::text("unexpected"));
    assert_eq!(scenario.state(), &State::confirm_install());
    assert_eq!(scenario.request(), request);
    assert!(!home.exists());
}
