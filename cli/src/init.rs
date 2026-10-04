mod choice;
mod error;
mod install;
mod postgres;
mod presentation;
mod prompt;
mod sqlite;
mod workflow;

use self::{
    error::InitError,
    postgres::PostgresSetup,
    workflow::{Initialization, Workflow},
};

use std::{path::Path, process::ExitCode};

use ::sql_fun_core::{InitializationDatasources, InitializeArgs, SqlFunArgs};
use ::statig::blocking::IntoStateMachineExt;

/// Initialize explicitly selected engine-specific development support.
///
/// # Errors
/// Reports incomplete choices, invalid selected configuration or setup failures.
pub async fn initialize(
    args: &SqlFunArgs,
    init_args: &InitializeArgs,
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
    let selection = SetupSelection::resolve(args, init_args)?;
    selection.check_choices(init_args)?;
    // Resolve all engine tasks before creating directories or installing anything.
    let plans = selection.prepare(args, init_args).await?;
    let mut context = Initialization::from(init_args);
    if init_args.sql_fun_home().is_none() {
        context.set_home(&args.initialization_home().map_err(anyhow::Error::from)?);
    }
    let mut machine = Workflow.state_machine();
    while let Some(request) = machine.state().request(&context) {
        let event = request.run()?;
        machine.handle_with_context(&event, &mut context);
        context.present();
    }
    context.finish()?;
    if machine.state().is_cancelled() {
        return Ok(ExitCode::FAILURE);
    }
    for plan in plans {
        plan.install(context.home()).await?;
    }
    Ok(ExitCode::SUCCESS)
}

/// Selection is explicit: datasource metadata describes candidates, never consent.
struct SetupSelection {
    engines: Vec<String>,
    datasource_names: Vec<String>,
    metadata: Option<InitializationDatasources>,
}
impl SetupSelection {
    fn resolve(common: &SqlFunArgs, args: &InitializeArgs) -> Result<Self, anyhow::Error> {
        let metadata = if common.metadata_file_is_explicit()
            || (args.engines().is_none() && common.metadata_file().is_ok())
        {
            Some(InitializationDatasources::load_from(
                &common.metadata_file()?,
            )?)
        } else {
            common.metadata_file_raw()?;
            None
        };
        let mut selection = Self {
            engines: Vec::new(),
            datasource_names: Vec::new(),
            metadata,
        };
        if let Some(engines) = args.engines() {
            for engine in engines {
                selection.add_engine(engine);
            }
        } else {
            let names = if let Some(names) = args.datasources() {
                names.to_vec()
            } else if let Some(metadata) = &selection.metadata {
                if metadata.names().is_empty() {
                    Vec::new()
                } else {
                    choice::select("Choose datasources to initialize", &metadata.names())?
                }
            } else {
                Vec::new()
            };
            if names.is_empty() {
                selection.engines = choice::select(
                    "Choose engines for development support",
                    &["postgres".to_owned(), "sqlite".to_owned()],
                )?;
            } else {
                for name in names {
                    let metadata = selection.metadata.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("selected datasource requires project metadata")
                    })?;
                    let engine = metadata.engine(&name)?;
                    let engine = if engine == "PostgreSQL" {
                        "postgres"
                    } else if engine == "sqlite" {
                        "sqlite"
                    } else {
                        return Err(anyhow::anyhow!(
                            "no initializer for datasource {name} engine {engine}"
                        ));
                    };
                    selection.add_engine(engine);
                    selection.datasource_names.push(name);
                }
            }
        }
        Ok(selection)
    }

    fn add_engine(&mut self, name: &str) {
        if !self.engines.contains(&name.to_owned()) {
            self.engines.push(name.to_owned());
        }
    }

    fn check_choices(&self, args: &InitializeArgs) -> Result<(), anyhow::Error> {
        if self.engines.contains(&"postgres".to_owned()) {
            PostgresSetup::check_choices(args)?;
        } else {
            anyhow::ensure!(
                !args.has_postgres_options(),
                "PostgreSQL options require selecting PostgreSQL setup"
            );
        }
        Ok(())
    }

    async fn prepare(
        &self,
        common: &SqlFunArgs,
        args: &InitializeArgs,
    ) -> Result<Vec<EngineSetup>, anyhow::Error> {
        let mut plans = Vec::new();
        for engine in &self.engines {
            if engine == "postgres" {
                plans.push(EngineSetup::postgres(
                    &PostgresSetup::prepare(
                        common,
                        args,
                        self.metadata.as_ref(),
                        &self.datasource_names,
                    )
                    .await?,
                ));
            } else {
                plans.push(EngineSetup::sqlite());
            }
        }
        Ok(plans)
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
