#[cfg(test)]
mod tests;

use super::{
    EngineSetup,
    error::InitError,
    presentation::{HomePathError, Notice},
    prompt::{PromptOutcome, PromptRequest, PromptResponse},
    selection::SetupSelection,
};
use ::sql_fun_core::{
    InitializeArgs, PostgresVersion, PostgresVersionInventory, SqlFunArgs,
    resolve_sql_fun_home_path,
};
use ::statig::Outcome;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
};

/// Navigation and confirmed answers belong to the conversation, not individual prompts.
pub(super) struct Initialization {
    common: SqlFunArgs,
    args: InitializeArgs,
    metadata_requested: bool,
    selection: Option<SetupSelection>,
    step: Step,
    history: Vec<Step>,
    answers: BTreeMap<Step, PromptResponse>,
    retry: Option<PromptResponse>,
    home: PathBuf,
    edit_home: bool,
    detected: Option<PostgresVersion>,
    plans: Vec<EngineSetup>,
    error: Option<InitError>,
    exit: ExitCode,
}
impl Initialization {
    pub(super) fn new(
        common: &SqlFunArgs,
        args: &InitializeArgs,
        metadata_requested: bool,
    ) -> Result<Self, anyhow::Error> {
        if args.assume_yes() {
            anyhow::ensure!(
                args.sql_fun_home().is_some(),
                "--yes requires --sql-fun-home"
            );
            anyhow::ensure!(
                args.engines().is_some() || args.datasources().is_some(),
                "--yes requires --engines or --datasource"
            );
        }
        let home = if let Some(home) = args.sql_fun_home() {
            home.clone()
        } else if let Some(home) = common.initialization_home_candidate() {
            home.to_path_buf()
        } else {
            PathBuf::from(shellexpand::tilde("~/.sql-fun").as_ref())
        };
        Ok(Self {
            common: common.clone(),
            args: args.clone(),
            metadata_requested,
            selection: None,
            step: Step::initial(),
            history: Vec::new(),
            answers: BTreeMap::new(),
            retry: None,
            home,
            edit_home: args.sql_fun_home().is_none(),
            detected: None,
            plans: Vec::new(),
            error: None,
            exit: ExitCode::SUCCESS,
        })
    }
    const fn selection(&self) -> &SetupSelection {
        self.selection
            .as_ref()
            .expect("selection loaded after home confirmation")
    }
    fn resolved_args(&self) -> InitializeArgs {
        let mut args = self.args.clone();
        if !args.docker() && args.database_url().is_none() {
            if self
                .answers
                .get(&Step::source())
                .is_some_and(|value| value.indices() == [0])
            {
                args.select_docker();
            } else if let Some(answer) = self.answers.get(&Step::connection())
                && let Some(connection) = answer.text_input()
            {
                args.set_database_url(connection);
            }
        }
        args
    }
    fn existing_server(&self) -> bool {
        self.args.database_url().is_some()
            || self
                .answers
                .get(&Step::source())
                .is_some_and(|value| value.indices() == [1])
    }
    fn version_options(&self) -> Vec<String> {
        if let Some(version) = self.detected {
            vec![version.to_string()]
        } else {
            PostgresVersionInventory::builtin()
                .targets()
                .into_iter()
                .map(|value| value.to_string())
                .collect()
        }
    }
    fn versions(&self) -> Result<Vec<PostgresVersion>, anyhow::Error> {
        let names = if let Some(names) = self.args.requested_versions() {
            names.to_vec()
        } else {
            self.step_names(Step::versions(), &self.version_options())
        };
        let versions = PostgresVersionInventory::builtin()
            .select(&names)
            .map_err(anyhow::Error::msg)?;
        if let Some(detected) = self.detected {
            anyhow::ensure!(
                versions == [detected],
                "selected versions must match connected PostgreSQL {detected}"
            );
        }
        Ok(versions)
    }
    fn step_names(&self, step: Step, options: &[String]) -> Vec<String> {
        if let Some(answer) = self.answers.get(&step) {
            answer.selected_names(options)
        } else {
            Vec::new()
        }
    }
    /// The single dependency table. Merely visiting a question never invalidates answers.
    fn remember(&mut self, answer: &PromptResponse) {
        if self.answers.get(&self.step) == Some(answer) {
            return;
        }
        for dependent in self.step.dependents() {
            self.answers.remove(dependent);
        }
        if self.step.invalidates_detection() {
            self.detected = None;
        }
        self.plans.clear();
        self.answers.insert(self.step, answer.clone());
    }
    fn visit(&mut self, step: Step) {
        if self.step != step {
            self.history.push(self.step);
            self.step = step;
        }
    }
    fn back(&mut self) {
        if let Some(step) = self.history.pop() {
            self.step = step;
        }
    }
    fn advance(&mut self) {
        let next = self.step.next();
        self.visit(next);
    }
    fn cancel(&mut self, interrupted: bool) -> Outcome<State> {
        self.exit = if interrupted {
            ExitCode::from(130)
        } else {
            ExitCode::SUCCESS
        };
        println!("Initialization cancelled.");
        Outcome::Transition(State::cancelled())
    }
    pub(super) fn finish(&mut self) -> Result<ExitCode, InitError> {
        if let Some(error) = self.error.take() {
            Err(error)
        } else {
            Ok(self.exit)
        }
    }
    /// Automatic preparation is read-only. Only interactive visits enter navigation history.
    pub(super) async fn prompt(&mut self) -> Result<PromptOutcome, anyhow::Error> {
        if !self.prepare_question().await? {
            return Ok(PromptOutcome::answer(&PromptResponse::selection(&[0])));
        }
        let request = self.step.request(self);
        let retry = self.retry.take();
        let previous = if self.step.is_review() {
            None
        } else if let Some(retry) = &retry {
            Some(retry)
        } else {
            self.answers.get(&self.step)
        };
        let suggestion = PromptResponse::text(&self.home.to_string_lossy());
        let previous = if previous.is_none() && self.step.is_home_editor() {
            Some(&suggestion)
        } else {
            previous
        };
        Ok(request.run(previous)?)
    }
    async fn prepare_question(&mut self) -> Result<bool, anyhow::Error> {
        loop {
            self.prepare().await?;
            if self.step.applicable(self) {
                return Ok(true);
            }
            if self.step.is_review() {
                return Ok(false);
            }
            self.step = self.step.next();
        }
    }
    async fn prepare(&mut self) -> Result<(), anyhow::Error> {
        if self.step.is_home_confirmation() && !self.args.assume_yes() {
            match resolve_sql_fun_home_path(&self.home) {
                Ok(path) if !path.exists() || path.is_dir() => {
                    self.home = path;
                }
                result => {
                    let error = match result {
                        Ok(path) => HomePathError::not_directory_error(&path),
                        Err(error) => HomePathError::resolution(&error),
                    };
                    Notice::invalid_home(error).show();
                    self.edit_home = true;
                    self.step = Step::home_editor();
                }
            }
        }
        if self.step.is_selection() && self.selection.is_none() {
            self.selection = Some(SetupSelection::load(
                &self.common,
                &self.args,
                self.metadata_requested,
            )?);
        }
        if self.step.needs_postgres_validation() {
            self.selection().check_choices(&self.args)?;
        }
        if self.step.is_versions() && self.selection().postgres() {
            // Reject unsupported explicit versions before any server contact.
            if let Some(names) = self.args.requested_versions() {
                PostgresVersionInventory::builtin()
                    .select(names)
                    .map_err(anyhow::Error::msg)?;
            }
            if self.existing_server() && self.detected.is_none() {
                let args = self.resolved_args();
                println!("Checking the explicitly selected PostgreSQL server version (read-only).");
                self.detected = Some(
                    crate::extract::detect_server(
                        args.database_url().expect("confirmed connection"),
                        args.timeout_seconds(),
                    )
                    .await?,
                );
            }
        }
        if self.step.is_review() {
            let versions = if self.selection().postgres() {
                self.versions()?
            } else {
                Vec::new()
            };
            self.plans =
                self.selection()
                    .prepare(&self.common, &self.resolved_args(), &versions)?;
            if self.args.assume_yes() {
                self.home = resolve_sql_fun_home_path(&self.home)?;
            } else {
                println!(
                    "Initialization review\nHome: {}\n{}",
                    self.home.display(),
                    self.selection().summary()
                );
                if self.selection().postgres() {
                    println!(
                        "PostgreSQL source: {}\nVersions: {}",
                        if self.existing_server() {
                            "Existing PostgreSQL server"
                        } else {
                            "Docker"
                        },
                        versions
                            .into_iter()
                            .map(|value| value.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
        Ok(())
    }
    fn accept(&mut self, answer: &PromptResponse) -> Result<Outcome<State>, InitError> {
        // All steps use the same change detection and answer storage.
        let step = self.step;
        if !step.validate(self, answer) {
            if !step.is_review() {
                self.retry = Some(answer.clone());
            }
            return Ok(Outcome::Handled);
        }
        if step.is_review() {
            if answer.indices() == [1] {
                return Ok(self.cancel(false));
            }
            return self.authorize();
        }
        self.remember(answer);
        if step.is_initial() && answer.confirmation() == Some(false) {
            return Ok(self.cancel(false));
        }
        if step.is_home_confirmation() && answer.confirmation() == Some(false) {
            self.edit_home = true;
            self.visit(Step::home_editor());
        } else if step.is_selection() {
            let names = self.step_names(step, &self.selection().options());
            self.selection
                .as_mut()
                .expect("loaded selection")
                .choose(&names)?;
            self.advance();
        } else {
            self.advance();
        }
        Ok(Outcome::Handled)
    }
    /// Home creation is the first mutation, reachable only after fresh authorization.
    fn authorize(&mut self) -> Result<Outcome<State>, InitError> {
        let existed = self.home.exists();
        if let Err(error) = std::fs::create_dir_all(&self.home) {
            if !self.args.assume_yes() && self.edit_home {
                Notice::home_create_failed(&self.home, error).show();
                self.plans.clear();
                self.visit(Step::home_editor());
                return Ok(Outcome::Handled);
            }
            return InitError::io(error);
        }
        if existed {
            Notice::home_exists(&self.home).show();
        } else {
            Notice::home_created(&self.home).show();
        }
        Ok(Outcome::Transition(State::authorized()))
    }
    pub(super) async fn install(&self) -> Result<(), InitError> {
        for plan in &self.plans {
            plan.install(&self.home).await?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Initial,
    HomeEditor,
    HomeConfirmation,
    Selection,
    Source,
    Connection,
    Versions,
    Review,
}
impl Step {
    const fn initial() -> Self {
        Self::Initial
    }
    const fn home_editor() -> Self {
        Self::HomeEditor
    }
    const fn home_confirmation() -> Self {
        Self::HomeConfirmation
    }
    const fn selection() -> Self {
        Self::Selection
    }
    const fn review() -> Self {
        Self::Review
    }
    const fn source() -> Self {
        Self::Source
    }
    const fn connection() -> Self {
        Self::Connection
    }
    const fn versions() -> Self {
        Self::Versions
    }
    const fn is_initial(self) -> bool {
        matches!(self, Self::Initial)
    }
    const fn is_home_editor(self) -> bool {
        matches!(self, Self::HomeEditor)
    }
    const fn is_home_confirmation(self) -> bool {
        matches!(self, Self::HomeConfirmation)
    }
    const fn is_selection(self) -> bool {
        matches!(self, Self::Selection)
    }
    const fn is_versions(self) -> bool {
        matches!(self, Self::Versions)
    }
    const fn is_review(self) -> bool {
        matches!(self, Self::Review)
    }
    const fn needs_postgres_validation(self) -> bool {
        matches!(self, Self::Source | Self::Review)
    }
    const fn invalidates_detection(self) -> bool {
        matches!(self, Self::Selection | Self::Source | Self::Connection)
    }
    const fn dependents(self) -> &'static [Self] {
        match self {
            Self::HomeEditor => &[Self::HomeConfirmation],
            Self::Selection => &[Self::Source, Self::Connection, Self::Versions],
            Self::Source => &[Self::Connection, Self::Versions],
            Self::Connection => &[Self::Versions],
            _ => &[],
        }
    }
    const fn next(self) -> Self {
        match self {
            Self::Initial => Self::HomeEditor,
            Self::HomeEditor => Self::home_confirmation(),
            Self::HomeConfirmation => Self::selection(),
            Self::Selection => Self::Source,
            Self::Source => Self::Connection,
            Self::Connection => Self::Versions,
            Self::Versions | Self::Review => Self::review(),
        }
    }
    fn applicable(self, context: &Initialization) -> bool {
        if context.args.assume_yes() {
            return false;
        }
        match self {
            Self::Initial | Self::HomeConfirmation | Self::Review => true,
            Self::HomeEditor => context.edit_home,
            Self::Selection => {
                context.args.engines().is_none() && context.args.datasources().is_none()
            }
            Self::Source => {
                context.selection().postgres()
                    && !context.args.docker()
                    && context.args.database_url().is_none()
            }
            Self::Connection => {
                context.selection().postgres()
                    && context.existing_server()
                    && context.args.database_url().is_none()
            }
            Self::Versions => {
                context.selection().postgres() && context.args.requested_versions().is_none()
            }
        }
    }
    fn request(self, context: &Initialization) -> PromptRequest {
        match self {
            Self::Initial => PromptRequest::confirm(include_str!("confirm_install.txt").trim_end()),
            Self::HomeEditor => {
                Notice::home_description().show();
                PromptRequest::text(include_str!("home_prompt.txt").trim_end())
            }
            Self::HomeConfirmation => PromptRequest::confirm(
                format!(
                    include_str!("confirm_home.txt"),
                    path = context.home.display()
                )
                .trim_end(),
            ),
            Self::Selection => PromptRequest::multi_select(
                context.selection().title(),
                &context.selection().options(),
            ),
            Self::Source => PromptRequest::select(
                "Choose the PostgreSQL builtin source",
                &["Docker".to_owned(), "Existing PostgreSQL server".to_owned()],
            ),
            Self::Connection => PromptRequest::text(
                "PostgreSQL libpq connection string (confirm to contact this server for read-only version detection; prefer a password file)",
            ),
            Self::Versions => PromptRequest::multi_select(
                if context.existing_server() {
                    "Choose the connected PostgreSQL version to initialize"
                } else {
                    "Choose PostgreSQL versions to initialize"
                },
                &context.version_options(),
            ),
            Self::Review => PromptRequest::authorization(),
        }
    }
    fn validate(self, context: &mut Initialization, answer: &PromptResponse) -> bool {
        match self {
            Self::Initial | Self::HomeConfirmation => answer.confirmation().is_some(),
            Self::HomeEditor => {
                let Some(input) = answer.text_input() else {
                    return false;
                };
                match Self::validate_home(input) {
                    Ok(path) => {
                        context.home = path;
                        true
                    }
                    Err(error) => {
                        Notice::invalid_home(error).show();
                        false
                    }
                }
            }
            Self::Connection => answer
                .text_input()
                .is_some_and(|value| !value.trim().is_empty()),
            Self::Review => answer.indices() == [0] || answer.indices() == [1],
            Self::Selection | Self::Source | Self::Versions => !answer.indices().is_empty(),
        }
    }
    fn validate_home(input: &str) -> Result<PathBuf, HomePathError> {
        let path = resolve_sql_fun_home_path(Path::new(input))
            .map_err(|error| HomePathError::resolution(&error))?;
        if path.exists() {
            if !path.is_dir() {
                return HomePathError::not_directory(&path);
            }
        } else if let Some(parent) = path.parent() {
            if !parent.is_dir() {
                if parent.exists() {
                    return HomePathError::not_directory(parent);
                }
                return HomePathError::parent_not_found(parent);
            }
        } else {
            return HomePathError::parent_unavailable();
        }
        Ok(path)
    }
}

pub(super) struct Workflow;
#[statig::state_machine(initial = "State::asking()", state(derive(Debug, PartialEq, Eq)))]
impl Workflow {
    #[state(superstate = "navigating")]
    fn asking(event: &PromptOutcome, context: &mut Initialization) -> Outcome<State> {
        let Some(answer) = event.response() else {
            return Outcome::Super;
        };
        match context.accept(answer) {
            Ok(outcome) => outcome,
            Err(error) => {
                context.error = Some(error);
                Outcome::Transition(State::failed())
            }
        }
    }
    #[superstate]
    fn navigating(event: &PromptOutcome, context: &mut Initialization) -> Outcome<State> {
        if event.is_back() {
            context.back();
            Outcome::Handled
        } else if event.is_cancel() {
            context.cancel(true)
        } else {
            Outcome::Super
        }
    }
    #[state]
    const fn authorized() -> Outcome<State> {
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
    pub(super) const fn is_asking(&self) -> bool {
        matches!(self, Self::Asking {})
    }
    pub(super) const fn is_authorized(&self) -> bool {
        matches!(self, Self::Authorized {})
    }
}
