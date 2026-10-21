use super::{EngineSetup, postgres::PostgresSetup};
use ::sql_fun_core::{InitializationDatasources, InitializeArgs, PostgresVersion, SqlFunArgs};

/// Selection is explicit: datasource metadata describes candidates, never consent.
pub(super) struct SetupSelection {
    engines: Vec<String>,
    datasource_names: Vec<String>,
    metadata: Option<InitializationDatasources>,
}
impl SetupSelection {
    pub(super) fn load(
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
        let mut selection = Self {
            engines: Vec::new(),
            datasource_names: Vec::new(),
            metadata,
        };
        if let Some(engines) = args.engines() {
            for engine in engines {
                selection.add_engine(engine);
            }
        } else if let Some(names) = args.datasources() {
            selection.choose(names)?;
        }
        Ok(selection)
    }
    pub(super) fn options(&self) -> Vec<String> {
        if let Some(metadata) = &self.metadata {
            let names = metadata.names();
            if !names.is_empty() {
                return names;
            }
        }
        vec!["postgres".to_owned(), "sqlite".to_owned()]
    }
    pub(super) fn title(&self) -> &str {
        if self.uses_datasources() {
            "Choose datasources to initialize"
        } else {
            "Choose engines for development support"
        }
    }
    fn uses_datasources(&self) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(|metadata| !metadata.names().is_empty())
    }
    pub(super) fn choose(&mut self, names: &[String]) -> Result<(), anyhow::Error> {
        self.engines.clear();
        self.datasource_names.clear();
        for name in names {
            if self.uses_datasources() {
                let metadata = self.metadata.as_ref().expect("requested metadata");
                let engine = metadata.engine(name)?;
                let engine = if engine == "PostgreSQL" {
                    "postgres"
                } else if engine == "sqlite" {
                    "sqlite"
                } else {
                    anyhow::bail!("no initializer for datasource {name} engine {engine}")
                };
                self.add_engine(engine);
                self.datasource_names.push(name.clone());
            } else {
                self.add_engine(name);
            }
        }
        Ok(())
    }
    pub(super) fn postgres(&self) -> bool {
        self.engines.contains(&"postgres".to_owned())
    }
    pub(super) fn summary(&self) -> String {
        format!(
            "Engines: {}\nDatasources: {}",
            self.engines.join(", "),
            self.datasource_names.join(", ")
        )
    }

    fn add_engine(&mut self, name: &str) {
        if !self.engines.contains(&name.to_owned()) {
            self.engines.push(name.to_owned());
        }
    }

    pub(super) fn check_choices(&self, args: &InitializeArgs) -> Result<(), anyhow::Error> {
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

    pub(super) fn prepare(
        &self,
        common: &SqlFunArgs,
        args: &InitializeArgs,
        versions: &[PostgresVersion],
    ) -> Result<Vec<EngineSetup>, anyhow::Error> {
        let mut plans = Vec::new();
        for engine in &self.engines {
            if engine == "postgres" {
                plans.push(EngineSetup::postgres(&PostgresSetup::prepare(
                    common,
                    args,
                    self.metadata.as_ref(),
                    &self.datasource_names,
                    versions,
                )?));
            } else {
                plans.push(EngineSetup::sqlite());
            }
        }
        Ok(plans)
    }
}
