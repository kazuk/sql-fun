mod choice;
mod conversation;
mod error;
mod flow;
mod home;
pub(crate) mod install;
mod postgres;
mod presentation;
mod prompt;
mod sqlite;
mod step;
#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;
mod theme;

use self::{
    error::InitError,
    home::Initialization,
    postgres::PostgresSetup,
    step::{PromptInput, TerminalInput},
};

use std::{path::Path, process::ExitCode};

use ::sql_fun_core::{InitializationDatasources, InitializeArgs, PostgresVersion, SqlFunArgs};

/// Initialize explicitly selected engine-specific development support.
///
/// # Errors
/// Reports incomplete choices, invalid selected configuration or setup failures.
pub async fn initialize(
    args: &SqlFunArgs,
    init_args: &InitializeArgs,
    metadata_file_from_cli: bool,
) -> Result<ExitCode, anyhow::Error> {
    if init_args.assume_yes() {
        anyhow::ensure!(
            init_args.sql_fun_home().is_some(),
            "--yes requires --sql-fun-home"
        );
        anyhow::ensure!(
            init_args.engines().is_some() || init_args.datasources().is_some(),
            "--yes requires --engines or --datasource"
        );
    }
    if !init_args.assume_yes() {
        println!("{}", include_str!("init/banner.txt"));
    }
    let mut state = InitializationState::new(args, init_args, metadata_file_from_cli);
    let completion = flow::collect(&mut state).await?;
    if completion.is_cancelled() {
        println!("Initialization cancelled.");
        return Ok(ExitCode::FAILURE);
    }
    state.home.create_directory()?;
    state
        .theme
        .as_ref()
        .expect("collected theme plan")
        .install()
        .await?;
    for plan in &state.plans {
        plan.install(state.home.home()).await?;
    }
    theme::validate_installed(state.home.home())?;
    Ok(ExitCode::SUCCESS)
}

/// All engines in an initialization share this same state, including branches.
struct InitializationState<'a> {
    common: &'a SqlFunArgs,
    args: InitializeArgs,
    metadata_requested: bool,
    home: Initialization,
    selection: Option<SetupSelection>,
    versions: Option<Vec<PostgresVersion>>,
    plans: Vec<EngineSetup>,
    input: Box<dyn PromptInput>,
    source: Option<bool>,
    connection: Option<String>,
    theme: Option<theme::ThemePlan>,
}

impl<'a> InitializationState<'a> {
    fn new(common: &'a SqlFunArgs, args: &InitializeArgs, metadata_requested: bool) -> Self {
        let mut home = Initialization::from(args);
        if args.sql_fun_home().is_none()
            && let Some(candidate) = common.initialization_home_candidate()
        {
            home.set_home(candidate);
        }
        Self {
            common,
            args: args.clone(),
            metadata_requested,
            home,
            selection: None,
            versions: None,
            plans: Vec::new(),
            input: Box::new(TerminalInput),
            source: None,
            connection: None,
            theme: None,
        }
    }
}

impl InitializationState<'_> {
    fn load_selection(&mut self) -> Result<(), anyhow::Error> {
        if self.selection.is_some() {
            return Ok(());
        }
        self.selection = Some(SetupSelection::load(
            self.common,
            &self.args,
            self.metadata_requested,
        )?);
        Ok(())
    }
    fn check_selection(&self) -> Result<(), anyhow::Error> {
        self.selection
            .as_ref()
            .expect("selected setup")
            .check_choices(&self.args)
    }
    fn discard_unselected_postgres(&mut self) {
        if !self.selection.as_ref().expect("selection").has_postgres() {
            self.source = None;
            self.connection = None;
            self.versions = None;
        }
    }
    fn choose_engines(&mut self) -> Result<(), InitError> {
        self.selection
            .as_mut()
            .expect("loaded selection")
            .choose_engines(&self.args, self.input.as_mut())?;
        self.discard_unselected_postgres();
        Ok(())
    }
    fn choose_datasources(&mut self) -> Result<(), anyhow::Error> {
        let previous = self
            .selection
            .as_ref()
            .expect("selection")
            .postgres_datasources();
        self.selection
            .as_mut()
            .expect("loaded selection")
            .choose_datasources(&self.args, self.input.as_mut())?;
        self.discard_unselected_postgres();
        if self
            .selection
            .as_ref()
            .expect("selection")
            .postgres_datasources()
            != previous
        {
            self.versions = None;
        }
        Ok(())
    }
    fn choose_postgres_source(&mut self) -> Result<(), InitError> {
        let selected = PostgresSetup::choose_source(&self.args, self.source, self.input.as_mut())?;
        if self.source.is_some_and(|previous| previous != selected) {
            self.connection = None;
            self.versions = None;
        }
        self.source = Some(selected);
        Ok(())
    }
    fn choose_connection(&mut self) -> Result<(), InitError> {
        let connection = PostgresSetup::choose_connection(
            &self.args,
            self.connection.as_deref(),
            self.input.as_mut(),
        )?;
        if self
            .connection
            .as_ref()
            .is_some_and(|previous| previous != &connection)
        {
            self.versions = None;
        }
        self.connection = Some(connection);
        Ok(())
    }
    fn uses_existing_server(&self) -> bool {
        self.source.unwrap_or(self.args.database_url().is_some())
    }
    fn effective_args(&self) -> InitializeArgs {
        let mut args = self.args.clone();
        if self.uses_existing_server() {
            if let Some(connection) = &self.connection {
                args.set_database_url(connection);
            }
        } else if self.source == Some(false) {
            args.select_docker();
        }
        args
    }
    async fn choose_versions(&mut self) -> Result<(), anyhow::Error> {
        self.versions = Some(
            PostgresSetup::choose_versions(
                &self.effective_args(),
                self.versions.as_deref(),
                self.input.as_mut(),
            )
            .await?,
        );
        Ok(())
    }
    fn prepare_plans(&mut self) -> Result<(), anyhow::Error> {
        self.plans.clear();
        let engines = self
            .selection
            .as_ref()
            .expect("selected setup")
            .engines
            .clone();
        for engine in engines {
            if engine == "postgres" {
                self.prepare_postgres()?;
            } else {
                self.prepare_sqlite();
            }
        }
        Ok(())
    }
    fn prepare_sqlite(&mut self) {
        self.plans.push(EngineSetup::sqlite());
    }
    #[cfg(test)]
    fn set_input(&mut self, input: impl PromptInput + 'static) {
        self.input = Box::new(input);
    }
    #[cfg(test)]
    const fn set_metadata_requested(&mut self, requested: bool) {
        self.metadata_requested = requested;
    }
    fn validate_complete(&self) -> Result<(), anyhow::Error> {
        anyhow::ensure!(
            self.home.confirmed(),
            "initialization home was not confirmed"
        );
        let selection = self
            .selection
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("initialization selection is missing"))?;
        anyhow::ensure!(
            !selection.engines.is_empty() && selection.engines.len() == self.plans.len(),
            "initialization settings are incomplete"
        );
        selection.check_choices(&self.args)?;
        Ok(())
    }
    fn prepare_postgres(&mut self) -> Result<(), anyhow::Error> {
        let selection = self.selection.as_ref().expect("selected setup");
        let setup = PostgresSetup::from_choices(
            self.common,
            &self.effective_args(),
            self.versions.as_ref().expect("explicit version choice"),
            selection.metadata.as_ref(),
            &selection.datasource_names,
        )?;
        self.plans.push(EngineSetup::postgres(&setup));
        Ok(())
    }
}

/// Selection is explicit: datasource metadata describes candidates, never consent.
struct SetupSelection {
    engines: Vec<String>,
    datasource_names: Vec<String>,
    metadata: Option<InitializationDatasources>,
}
impl SetupSelection {
    fn load(
        common: &SqlFunArgs,
        args: &InitializeArgs,
        metadata_file_from_cli: bool,
    ) -> Result<Self, anyhow::Error> {
        let metadata = if metadata_file_from_cli || args.datasources().is_some() {
            Some(InitializationDatasources::load_from(
                &common.metadata_file()?,
            )?)
        } else {
            None
        };
        Ok(Self {
            engines: Vec::new(),
            datasource_names: Vec::new(),
            metadata,
        })
    }

    fn has_postgres(&self) -> bool {
        self.engines.iter().any(|engine| engine == "postgres")
    }

    fn postgres_datasources(&self) -> Vec<String> {
        let Some(metadata) = &self.metadata else {
            return Vec::new();
        };
        let mut names = self
            .datasource_names
            .iter()
            .filter(|name| {
                metadata
                    .engine(name)
                    .is_ok_and(|engine| engine == "PostgreSQL")
            })
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn uses_datasources(&self, args: &InitializeArgs) -> bool {
        args.engines().is_none()
            && (args.datasources().is_some()
                || self
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| !metadata.names().is_empty()))
    }

    fn choose_engines(
        &mut self,
        args: &InitializeArgs,
        input: &mut dyn PromptInput,
    ) -> Result<(), InitError> {
        if let Some(engines) = args.engines() {
            for engine in engines {
                self.add_engine(engine);
            }
        } else {
            self.engines = choice::select_retained(
                "Choose engines for development support",
                &["postgres".to_owned(), "sqlite".to_owned()],
                &self.engines,
                |request| input.answer(request),
            )?;
        }
        Ok(())
    }

    fn choose_datasources(
        &mut self,
        args: &InitializeArgs,
        input: &mut dyn PromptInput,
    ) -> Result<(), anyhow::Error> {
        let metadata = self
            .metadata
            .as_ref()
            .expect("explicitly requested metadata");
        let names = if let Some(names) = args.datasources() {
            names.to_vec()
        } else {
            choice::select_retained(
                "Choose datasources to initialize",
                &metadata.names(),
                &self.datasource_names,
                |request| input.answer(request),
            )?
        };
        self.engines.clear();
        self.datasource_names.clear();
        for name in names {
            self.add_datasource(&name)?;
        }
        Ok(())
    }

    fn add_datasource(&mut self, name: &str) -> Result<(), anyhow::Error> {
        let metadata = self
            .metadata
            .as_ref()
            .expect("explicitly requested metadata");
        let engine = metadata.engine(name)?;
        let engine = if engine == "PostgreSQL" {
            "postgres"
        } else if engine == "sqlite" {
            "sqlite"
        } else {
            return Err(anyhow::anyhow!(
                "no initializer for datasource {name} engine {engine}"
            ));
        };
        self.add_engine(engine);
        self.datasource_names.push(name.to_owned());
        Ok(())
    }

    fn add_engine(&mut self, name: &str) {
        if !self.engines.contains(&name.to_owned()) {
            self.engines.push(name.to_owned());
        }
    }

    fn check_choices(&self, args: &InitializeArgs) -> Result<(), anyhow::Error> {
        if self.has_postgres() {
            PostgresSetup::check_choices(args)?;
        } else {
            anyhow::ensure!(
                !args.has_postgres_options(),
                "PostgreSQL options require selecting PostgreSQL setup"
            );
        }
        Ok(())
    }
}

enum EngineSetup {
    Postgres(Box<PostgresSetup>),
    Sqlite,
}
impl EngineSetup {
    fn postgres(setup: &PostgresSetup) -> Self {
        Self::Postgres(Box::new(setup.clone()))
    }
    const fn sqlite() -> Self {
        Self::Sqlite
    }
    async fn install(&self, home: &Path) -> Result<(), InitError> {
        match self {
            Self::Postgres(setup) => setup.install(home).await,
            Self::Sqlite => {
                sqlite::initialize(home);
                Ok(())
            }
        }
    }
}
