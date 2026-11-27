use crate::{Api, InitializedSchema, SchemaLoadError, SchemaSelection};
use std::path::PathBuf;

use ::sql_fun_core::SqlFunArgs;
use ::sql_fun_server_api::{SqlFunServerApiError, payloads::InitializeSchemaArgs};

/// Validated schema selection using configuration restored by core.
pub(super) struct Initialization {
    home: PathBuf,
    selection: SchemaSelection,
    rewrite_options: ::sql_fun_core::RewriteOptions,
    lint_configuration: ::sql_fun_core::LintConfigurations,
    lint_path: Option<PathBuf>,
}

impl Initialization {
    pub(super) fn from_request(
        request: &InitializeSchemaArgs,
    ) -> Result<Self, SqlFunServerApiError> {
        Self::resolve(request).map_err(SqlFunServerApiError::new_schema_context_initialize_failed)
    }

    fn resolve(request: &InitializeSchemaArgs) -> Result<Self, SchemaLoadError> {
        if request.datasource().is_empty() {
            return Err(SchemaLoadError::source(&"explicit datasource is required"));
        }
        let args = SqlFunArgs::from_environments(request.environments())
            .map_err(|error| SchemaLoadError::source(&error))?;
        let metadata = args
            .selected_metadata(request.datasource())
            .map_err(|error| SchemaLoadError::source(&error))?;
        let home = args
            .sql_fun_home()
            .map_err(|error| SchemaLoadError::source(&error))?;
        let level = request.level();
        let files = if level.includes_extensions() {
            let directory = args
                .postgres_extensions_dir(&metadata)
                .map_err(|error| SchemaLoadError::source(&error))?;
            args.postgres_extensions(&metadata)
                .scheduled_scripts(&directory)
                .map_err(|error| SchemaLoadError::source(&error))?
        } else {
            Vec::new()
        };
        let mut selection = SchemaSelection::for_level(
            request.datasource(),
            &args,
            &metadata,
            &[],
            request.schema_dump().as_deref(),
            request.expected_hash().as_deref(),
            level,
        )?;
        selection.set_extension_scripts(files);
        let (lint_configuration, lint_path) = if level.includes_project() {
            (
                metadata
                    .load_lint_configuration()
                    .map_err(|error| SchemaLoadError::source(&error))?,
                metadata
                    .lint_configuration()
                    .map(std::path::Path::to_path_buf),
            )
        } else {
            (::sql_fun_core::LintConfigurations::default(), None)
        };
        Ok(Self {
            home,
            selection,
            rewrite_options: *metadata.rewrite_options(),
            lint_configuration,
            lint_path,
        })
    }

    pub(super) const fn rewrite_options(&self) -> &::sql_fun_core::RewriteOptions {
        &self.rewrite_options
    }

    pub(super) const fn lint_configuration(&self) -> &::sql_fun_core::LintConfigurations {
        &self.lint_configuration
    }

    pub(super) fn lint_path(&self) -> Option<&std::path::Path> {
        self.lint_path.as_deref()
    }

    pub(super) async fn initialize(
        &self,
        api: &Api,
    ) -> Result<InitializedSchema, SqlFunServerApiError> {
        let service = api
            .schema_cache_service(&self.home)
            .map_err(SqlFunServerApiError::new_schema_context_initialize_failed)?;
        service
            .initialize(&self.selection)
            .await
            .map_err(SqlFunServerApiError::new_schema_context_initialize_failed)
    }
}

#[cfg(test)]
mod tests {
    use crate::session::initialization::Initialization;
    use crate::{
        Api, schema_cache::test_helpers::SchemaProject,
        test_helpers::extension_initialization_request,
    };
    use sql_fun_sqlast::{
        context::{BaseParseContext, SearchPathRead},
        names::{FullName, TableName},
    };

    #[tokio::test]
    async fn extension_initialization_applies_updates_and_destination_controls() {
        let project = SchemaProject::new();
        let directory = project.home().join("postgres/17/extensions");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("example.control"),
            "schema = ignored\ndefault_version = 1.2\n",
        )
        .unwrap();
        std::fs::write(
            directory.join("example--1.0.control"),
            "schema = 'Extension Schema'\n",
        )
        .unwrap();
        std::fs::write(
            directory.join("example--1.0.sql"),
            "CREATE TABLE ext_table (id int4);",
        )
        .unwrap();
        std::fs::write(
            directory.join("example--1.1.control"),
            "schema = another_schema\n",
        )
        .unwrap();
        std::fs::write(
            directory.join("example--1.0--1.1.sql"),
            "ALTER TABLE @extschema@.ext_table RENAME TO updated_table;",
        )
        .unwrap();
        std::fs::write(
            directory.join("example--1.1--1.2.sql"),
            "ALTER TABLE @extschema@.updated_table OWNER TO @extowner@;",
        )
        .unwrap();
        let manifest = project.home().join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).unwrap().replace(
            "[package.metadata.database.products]",
            "[package.metadata.database.products]\nroles = ['postgres', 'Install Owner']",
        );
        std::fs::write(&manifest, text).unwrap();
        let mut request = project.rpc_request("-- unused");
        request["schema_level"] = serde_json::json!("extensions");
        request["environments"]["SQL_FUN_EXTENSIONS"] = serde_json::json!("example");
        let request = serde_json::from_value(request).unwrap();
        let api = Api {
            schema_cache: std::sync::OnceLock::new(),
        };
        let default = Initialization::from_request(&request)
            .unwrap()
            .initialize(&api)
            .await
            .unwrap();

        let text = std::fs::read_to_string(&manifest).unwrap().replace(
            "[package.metadata.database.products]",
            "[package.metadata.database.products]\nextension_install_owner = 'Install Owner'",
        );
        std::fs::write(&manifest, text).unwrap();
        let specified = Initialization::from_request(&request)
            .unwrap()
            .initialize(&api)
            .await
            .unwrap();
        let table = TableName::from(FullName::with_schema("Extension Schema", "updated_table"));
        for (initialized, owner) in [(&default, "postgres"), (&specified, "Install Owner")] {
            let base = initialized.base_context();
            assert_eq!(base.get_default_schema().unwrap().as_str(), "public");
            assert_eq!(
                base.get_table(&table).unwrap().owner().unwrap().name(),
                Some(owner)
            );
        }
        assert_ne!(default.key(), specified.key());
    }
    #[tokio::test]
    async fn extension_initialization_interleaves_updates_and_prerequisites() {
        let project = SchemaProject::new();
        let request = extension_initialization_request(
            &project,
            "a",
            &[
                ("a.control", "default_version = 3\nschema = 'A Schema'"),
                ("a--1.sql", "CREATE TABLE installed (id int4 PRIMARY KEY);"),
                ("a--2.control", "requires = 'b'"),
                (
                    "a--1--2.sql",
                    "ALTER TABLE @extschema:b@.dependent RENAME TO updated;",
                ),
                (
                    "a--2--3.sql",
                    "ALTER TABLE @extschema@.installed RENAME TO finished;",
                ),
                (
                    "b.control",
                    "default_version = 1\nschema = 'B Schema'\nrequires = 'a'",
                ),
                (
                    "b--1.sql",
                    "CREATE TABLE dependent (id int4 REFERENCES @extschema:a@.installed (id));",
                ),
            ],
        );
        let api = Api {
            schema_cache: std::sync::OnceLock::new(),
        };
        let initialized = Initialization::from_request(&request)
            .unwrap()
            .initialize(&api)
            .await
            .unwrap();

        assert!(
            initialized
                .analysis()
                .diagnostics()
                .iter()
                .all(|layer| layer.problems().is_empty()),
            "extension scripts should analyze without diagnostics"
        );
        let base = initialized.base_context();
        for (schema, name) in [("A Schema", "finished"), ("B Schema", "updated")] {
            let table = TableName::from(FullName::with_schema(schema, name));
            assert!(base.get_table(&table).is_some(), "{schema}.{name}");
        }
    }

    #[tokio::test]
    async fn earthdistance_uses_implicit_cubes_installation_schema() {
        let project = SchemaProject::new();
        let request = extension_initialization_request(
            &project,
            "earthdistance",
            &[
                (
                    "cube.control",
                    "default_version = 1\nschema = 'Cube Schema'",
                ),
                (
                    "cube--1.sql",
                    "CREATE TABLE cube_definition (id int4 PRIMARY KEY);",
                ),
                (
                    "earthdistance.control",
                    "default_version = 1\nrequires = 'cube'",
                ),
                (
                    "earthdistance--1.sql",
                    "CREATE TABLE earth_definition (id int4 REFERENCES @extschema:cube@.cube_definition (id));",
                ),
            ],
        );
        let api = Api {
            schema_cache: std::sync::OnceLock::new(),
        };
        let initialized = Initialization::from_request(&request)
            .unwrap()
            .initialize(&api)
            .await
            .unwrap();

        assert!(
            initialized
                .analysis()
                .diagnostics()
                .iter()
                .all(|layer| layer.problems().is_empty()),
            "extension scripts should analyze without diagnostics"
        );
        let base = initialized.base_context();
        for (schema, name) in [
            ("Cube Schema", "cube_definition"),
            ("public", "earth_definition"),
        ] {
            let table = TableName::from(FullName::with_schema(schema, name));
            assert!(base.get_table(&table).is_some(), "{schema}.{name}");
        }
    }
}
