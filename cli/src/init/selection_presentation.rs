//! Decision context derived only from explicitly requested configuration.
use crate::init::SetupSelection;
use ::sql_fun_core::SqlFunArgs;
use std::path::Path;

pub(super) const POSTGRES_DESCRIPTION: &str = "PostgreSQL: install descriptions of PostgreSQL's built-in database objects for SQL analysis; source and version questions follow.";
pub(super) const SQLITE_DESCRIPTION: &str = "SQLite: development-support setup is still under development; no SQLite engine data will be installed.";
const UNAVAILABLE: &str = "not available; selected configuration will be validated";

impl SetupSelection {
    pub(super) fn engine_explanation(&self, home: &Path) -> String {
        let mut lines = self.metadata_heading();
        if self
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.names().is_empty())
        {
            lines.push("No named datasources are defined in this file. Choose database engines to prepare instead.".to_owned());
        }
        lines.extend([
            format!("Choose which database engines to prepare support for in {}.", home.display()),
            POSTGRES_DESCRIPTION.to_owned(), SQLITE_DESCRIPTION.to_owned(),
            "Either choice still requires a valid diagnostic theme. You may select both. This does not change your application's database configuration.".to_owned(),
        ]);
        lines.join("\n")
    }

    fn metadata_heading(&self) -> Vec<String> {
        self.metadata_source
            .iter()
            .map(|path| format!("Configuration source: {}", path.display()))
            .collect()
    }

    fn datasource_description(&self, common: &SqlFunArgs, name: &str) -> String {
        let metadata = self.metadata.as_ref().expect("requested metadata");
        let engine = metadata.engine(name).unwrap_or(UNAVAILABLE);
        if engine == "PostgreSQL" {
            let version = if let Ok(selected) = metadata.selected_metadata(name)
                && let Ok(version) = common.postgres_version(&selected)
            {
                version.to_string()
            } else {
                UNAVAILABLE.to_owned()
            };
            format!("{name}: engine {engine}; PostgreSQL target {version}")
        } else {
            format!("{name}: engine {engine}")
        }
    }

    pub(super) fn datasource_explanation(&self, common: &SqlFunArgs) -> String {
        let mut lines = self.metadata_heading();
        lines.push("A datasource is a named database configuration in this file. Choose which configurations to prepare local analysis support for; this does not modify the file or connect to their application databases. Configurations using the same PostgreSQL version share one local installation. A valid diagnostic theme is required for either engine.".to_owned());
        let metadata = self.metadata.as_ref().expect("requested metadata");
        for name in metadata.names() {
            lines.push(self.datasource_description(common, &name));
        }
        if metadata.names().iter().any(|name| {
            metadata
                .engine(name)
                .is_ok_and(|engine| engine == "PostgreSQL")
        }) {
            lines.push(POSTGRES_DESCRIPTION.to_owned());
        }
        if metadata
            .names()
            .iter()
            .any(|name| metadata.engine(name).is_ok_and(|engine| engine == "sqlite"))
        {
            lines.push(SQLITE_DESCRIPTION.to_owned());
        }
        lines.join("\n")
    }

    pub(super) fn version_requirements(&self, common: &SqlFunArgs) -> String {
        let mut lines = self
            .postgres_datasources()
            .iter()
            .map(|name| self.datasource_description(common, name))
            .collect::<Vec<_>>();
        if let Ok(Some(version)) = common.postgres_version_override() {
            lines.push(format!("Explicit PostgreSQL version override: {version}"));
        }
        if !lines.is_empty() {
            lines.push("Selected versions must cover these configured targets; configuration does not select versions for you.".to_owned());
        }
        lines.join("\n")
    }

    pub(super) fn review(&self, common: &SqlFunArgs) -> String {
        let mut lines = self.metadata_heading();
        if !self.datasource_names.is_empty() {
            lines.push("Selected datasources:".to_owned());
            lines.extend(
                self.datasource_names
                    .iter()
                    .map(|name| self.datasource_description(common, name)),
            );
        }
        lines.push(format!(
            "Engines (execution order): {}",
            self.engines.join(", ")
        ));
        lines.join("\n")
    }
}
