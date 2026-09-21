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
            context.home_input.clone_from(input);
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
        let Event::Answer(PromptResponse::MultiSelect(indices)) = event else {
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

    fn confirm_install_request(context: &Initialization) -> Request {
        Request::Prompt(
            PromptRequest::confirm(include_str!("confirm_install.txt").trim_end())
                .with_automatic(context.yes.then_some(PromptResponse::Confirm(true))),
        )
    }

    const fn prepare_home_request() -> Request {
        Request::Advance
    }

    fn confirm_create_request(context: &Initialization) -> Request {
        Request::Prompt(
            PromptRequest::confirm(
                format!(
                    include_str!("confirm_create.txt"),
                    path = context.home.display()
                )
                .trim_end(),
            )
            .with_automatic(context.yes.then_some(PromptResponse::Confirm(true))),
        )
    }

    const fn create_home_request() -> Request {
        Request::Advance
    }

    fn edit_home_request(context: &Initialization) -> Request {
        Request::Prompt(
            PromptRequest::text(
                include_str!("home_prompt.txt").trim_end(),
                &context.default_home,
            )
            .with_initial_text(&context.home_input),
        )
    }

    const fn create_edited_home_request() -> Request {
        Request::Advance
    }

    const fn prepare_versions_request() -> Request {
        Request::Advance
    }

    fn select_versions_request(context: &Initialization) -> Request {
        let options: Vec<_> = PostgresVersions::ALL
            .iter()
            .map(ToString::to_string)
            .collect();
        let defaults: Vec<_> = PostgresVersions::ALL
            .iter()
            .enumerate()
            .filter_map(|(index, version)| context.versions.contains(version).then_some(index))
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
        Request::Prompt(
            PromptRequest::multi_select(
                include_str!("versions_prompt.txt").trim_end(),
                &options,
                &defaults,
            )
            .with_automatic(automatic),
        )
    }

    const fn versions_selected_request() -> Option<Request> {
        None
    }

    const fn cancelled_request() -> Option<Request> {
        None
    }

    const fn failed_request() -> Option<Request> {
        None
    }

    pub(super) const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled {})
    }
}
