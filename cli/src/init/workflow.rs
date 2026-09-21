#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

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
    home_input: String,
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
            home_input: String::new(),
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

impl Event {
    const fn response(&self) -> Option<&PromptResponse> {
        if let Self::Answer(response) = self {
            Some(response)
        } else {
            None
        }
    }

    fn confirmation(&self) -> Option<bool> {
        self.response().and_then(PromptResponse::confirmation)
    }

    fn text_input(&self) -> Option<&str> {
        self.response().and_then(PromptResponse::text_input)
    }

    fn selected_indices(&self) -> Option<&[usize]> {
        self.response().and_then(PromptResponse::selected_indices)
    }
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
        match event.confirmation() {
            Some(true) => Outcome::Transition(State::prepare_home()),
            Some(false) => Outcome::Transition(State::cancelled()),
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
        match event.confirmation() {
            Some(true) => Outcome::Transition(State::create_home()),
            Some(false) => Outcome::Transition(State::edit_home()),
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
        let Some(input) = event.text_input() else {
            return Outcome::Handled;
        };
        let path = PathBuf::from(input);
        let error = if !path.is_absolute() {
            Some(HomePathError::NotAbsolute)
        } else if path.exists() {
            None
        } else if let Some(parent) = path.parent() {
            if parent.exists() {
                None
            } else {
                Some(HomePathError::ParentNotFound(parent.to_path_buf()))
            }
        } else {
            Some(HomePathError::ParentUnavailable)
        };
        if let Some(error) = error {
            input.clone_into(&mut context.home_input);
            context.notices.push(Notice::InvalidHome(error));
            return Outcome::Handled;
        }
        context.home_input.clear();
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
        let Some(indices) = event.selected_indices() else {
            return Outcome::Handled;
        };
        if indices.is_empty() && !context.yes {
            context.versions.clear();
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
        match self {
            Self::ConfirmInstall {} => Some(Self::confirm_install_request(context)),
            Self::PrepareHome {} => Some(Self::prepare_home_request()),
            Self::ConfirmCreate {} => Some(Self::confirm_create_request(context)),
            Self::CreateHome {} => Some(Self::create_home_request()),
            Self::EditHome {} => Some(Self::edit_home_request(context)),
            Self::CreateEditedHome {} => Some(Self::create_edited_home_request()),
            Self::PrepareVersions {} => Some(Self::prepare_versions_request()),
            Self::SelectVersions {} => Some(Self::select_versions_request(context)),
            Self::VersionsSelected {} => Self::versions_selected_request(),
            Self::Cancelled {} => Self::cancelled_request(),
            Self::Failed {} => Self::failed_request(),
        }
    }

    pub(super) const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled {})
    }
}

impl State {
    fn confirm_install_request(context: &Initialization) -> Request {
        let automatic = if context.yes {
            Some(PromptResponse::confirm(true))
        } else {
            None
        };
        Request::Prompt(
            PromptRequest::confirm(include_str!("confirm_install.txt").trim_end())
                .with_automatic(automatic),
        )
    }
}

#[cfg(test)]
mod test_confirm_install_request {
    use super::test_helpers::initialization;
    use super::{PromptRequest, PromptResponse, Request, State};

    #[rstest::rstest]
    #[case::interactive(false, None)]
    #[case::assume_yes(true, Some(PromptResponse::confirm(true)))]
    fn requests_confirmation(#[case] yes: bool, #[case] automatic: Option<PromptResponse>) {
        assert_eq!(
            State::confirm_install_request(&initialization(yes)),
            Request::Prompt(
                PromptRequest::confirm(include_str!("confirm_install.txt").trim_end())
                    .with_automatic(automatic)
            ),
        );
    }
}

impl State {
    const fn prepare_home_request() -> Request {
        Request::Advance
    }
}

#[cfg(test)]
mod test_prepare_home_request {
    use super::{Request, State};

    #[test]
    fn prepare_home_requests_advance() {
        assert_eq!(State::prepare_home_request(), Request::Advance);
    }
}

impl State {
    fn confirm_create_request(context: &Initialization) -> Request {
        let automatic = if context.yes {
            Some(PromptResponse::confirm(true))
        } else {
            None
        };
        Request::Prompt(
            PromptRequest::confirm(
                format!(
                    include_str!("confirm_create.txt"),
                    path = context.home.display()
                )
                .trim_end(),
            )
            .with_automatic(automatic),
        )
    }
}

#[cfg(test)]
mod test_confirm_create_request {
    use super::test_helpers::initialization;
    use super::{PromptRequest, PromptResponse, Request, State};

    #[rstest::rstest]
    #[case::interactive(false, None)]
    #[case::assume_yes(true, Some(PromptResponse::confirm(true)))]
    fn requests_confirmation(#[case] yes: bool, #[case] automatic: Option<PromptResponse>) {
        assert_eq!(
            State::confirm_create_request(&initialization(yes)),
            Request::Prompt(
                PromptRequest::confirm("Create the chosen/home?").with_automatic(automatic)
            ),
        );
    }
}

impl State {
    const fn create_home_request() -> Request {
        Request::Advance
    }
}

#[cfg(test)]
mod test_create_home_request {
    use super::{Request, State};

    #[test]
    fn create_home_requests_advance() {
        assert_eq!(State::create_home_request(), Request::Advance);
    }
}

impl State {
    fn edit_home_request(context: &Initialization) -> Request {
        Request::Prompt(
            PromptRequest::text(
                include_str!("home_prompt.txt").trim_end(),
                &context.default_home,
            )
            .with_initial_text(&context.home_input),
        )
    }
}

#[cfg(test)]
mod test_edit_home_request {
    use super::test_helpers::initialization;
    use super::{PromptRequest, Request, State};

    #[rstest::rstest]
    #[case::first_attempt("")]
    #[case::invalid_input("invalid input")]
    fn retains_input_and_original_default(#[case] input: &str) {
        let mut context = initialization(false);
        input.clone_into(&mut context.home_input);
        assert_eq!(
            State::edit_home_request(&context),
            Request::Prompt(
                PromptRequest::text(include_str!("home_prompt.txt").trim_end(), "original/home")
                    .with_initial_text(input),
            ),
        );
    }
}

impl State {
    const fn create_edited_home_request() -> Request {
        Request::Advance
    }
}

#[cfg(test)]
mod test_create_edited_home_request {
    use super::{Request, State};

    #[test]
    fn create_edited_home_requests_advance() {
        assert_eq!(State::create_edited_home_request(), Request::Advance);
    }
}

impl State {
    const fn prepare_versions_request() -> Request {
        Request::Advance
    }
}

#[cfg(test)]
mod test_prepare_versions_request {
    use super::{Request, State};

    #[test]
    fn prepare_versions_requests_advance() {
        assert_eq!(State::prepare_versions_request(), Request::Advance);
    }
}

impl State {
    fn select_versions_request(context: &Initialization) -> Request {
        let options: Vec<_> = PostgresVersions::ALL
            .iter()
            .map(ToString::to_string)
            .collect();
        let defaults: Vec<_> = PostgresVersions::ALL
            .iter()
            .enumerate()
            .filter_map(|(index, version)| {
                if context.versions.contains(version) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        let automatic = if context.yes {
            Some(PromptResponse::multi_select(
                &context
                    .versions
                    .iter()
                    .filter_map(|version| {
                        PostgresVersions::ALL
                            .iter()
                            .position(|option| option == version)
                    })
                    .collect::<Vec<_>>(),
            ))
        } else {
            None
        };
        Request::Prompt(
            PromptRequest::multi_select(
                include_str!("versions_prompt.txt").trim_end(),
                &options,
                &defaults,
            )
            .with_automatic(automatic),
        )
    }
}

#[cfg(test)]
mod test_select_versions_request {
    use super::test_helpers::initialization;
    use super::{PostgresVersions, PromptRequest, PromptResponse, Request, State};

    fn version_prompt(defaults: &[usize]) -> PromptRequest {
        PromptRequest::multi_select(
            include_str!("versions_prompt.txt").trim_end(),
            &[
                "PostgreSQL Version 16".to_owned(),
                "PostgreSQL Version 17".to_owned(),
                "PostgreSQL Version 18".to_owned(),
            ],
            defaults,
        )
    }

    #[rstest::rstest]
    #[case::all_supported(&PostgresVersions::ALL, &[0, 1, 2])]
    #[case::option_order_without_duplicates(
        &[PostgresVersions::Postgres18, PostgresVersions::Postgres16, PostgresVersions::Postgres18],
        &[0, 2],
    )]
    #[case::empty_selection_when_reprompting(&[], &[])]
    fn interactive_defaults(#[case] versions: &[PostgresVersions], #[case] defaults: &[usize]) {
        let mut context = initialization(false);
        versions.clone_into(&mut context.versions);
        assert_eq!(
            State::select_versions_request(&context),
            Request::Prompt(version_prompt(defaults)),
        );
    }

    #[test]
    fn select_versions_automatic_answer_preserves_order_and_duplicates() {
        let mut context = initialization(true);
        context.versions = vec![
            PostgresVersions::Postgres18,
            PostgresVersions::Postgres16,
            PostgresVersions::Postgres18,
        ];
        assert_eq!(
            State::select_versions_request(&context),
            Request::Prompt(
                version_prompt(&[0, 2])
                    .with_automatic(Some(PromptResponse::multi_select(&[2, 0, 2]))),
            ),
        );
    }
}

impl State {
    const fn versions_selected_request() -> Option<Request> {
        None
    }
}

#[cfg(test)]
mod test_versions_selected_request {
    use super::State;

    #[test]
    fn versions_selected_requests_no_further_work() {
        assert_eq!(State::versions_selected_request(), None);
    }
}

impl State {
    const fn cancelled_request() -> Option<Request> {
        None
    }
}

#[cfg(test)]
mod test_cancelled_request {
    use super::State;

    #[test]
    fn cancelled_requests_no_further_work() {
        assert_eq!(State::cancelled_request(), None);
    }
}

impl State {
    const fn failed_request() -> Option<Request> {
        None
    }
}

#[cfg(test)]
mod test_failed_request {
    use super::State;

    #[test]
    fn failed_requests_no_further_work() {
        assert_eq!(State::failed_request(), None);
    }
}
