use crate::{
    Api, InitializedSchema, SchemaSelection,
    core_wrapping::{CoreDialect, CorePostgresVersion},
};
use std::{collections::HashMap, path::PathBuf};

use ::sql_fun_core::{InitializationDatasources, PostgresEngineVersion, SqlDialect, SqlFunArgs};
use ::sql_fun_server_api::{InitializeSchemaArgs, SqlFunServerApiError};

/// Resolve only assets explicitly selected by this request. The builder receives
/// a controlled environment, never the server's ambient environment.
pub(super) struct Initialization {
    home: PathBuf,
    selection: SchemaSelection,
}

impl Initialization {
    pub(super) fn from_request(
        request: &InitializeSchemaArgs,
    ) -> Result<Self, SqlFunServerApiError> {
        Self::resolve(request).map_err(SqlFunServerApiError::new_schema_context_initialize_failed)
    }

    fn selected_version(
        request: &InitializeSchemaArgs,
    ) -> Result<PostgresEngineVersion, crate::SchemaLoadError> {
        use crate::SchemaLoadError;
        let version: PostgresEngineVersion =
            CorePostgresVersion::try_from(request.engine_version())
                .map_err(|error| SchemaLoadError::source(&error))?
                .into();
        let version_text = version.to_string();
        if version_text.trim().is_empty()
            || version_text == "."
            || version_text == ".."
            || version_text.contains(['/', '\\', '\0'])
        {
            return Err(SchemaLoadError::source(
                &"explicit engine version must be an installation name",
            ));
        }
        Ok(version)
    }

    fn resolve(request: &InitializeSchemaArgs) -> Result<Self, crate::SchemaLoadError> {
        use crate::SchemaLoadError;
        let invalid = |message: &str| SchemaLoadError::source(&message);
        let env = request.environments();
        for key in env.keys() {
            if key != "SQL_FUN_HOME" && key != "SQL_FUN_METADATA_FILE" {
                return Err(invalid(&format!(
                    "unsupported initialization environment: {key}"
                )));
            }
        }
        let required = |key: &str| {
            env.get(key)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| invalid(&format!("explicit {key} is required")))
        };
        let home = required("SQL_FUN_HOME")?;
        let metadata_path = required("SQL_FUN_METADATA_FILE")?;
        if request.datasource().is_empty() || request.schema_dump().as_os_str().is_empty() {
            return Err(invalid("explicit datasource and schema_dump are required"));
        }
        // Request settings are authoritative only when no conflicting metadata
        // setting exists. Resolve the named datasource without scanning siblings.
        let declarations =
            InitializationDatasources::load_from(std::path::Path::new(metadata_path))
                .map_err(|error| SchemaLoadError::source(&error))?;
        if declarations
            .engine(request.datasource())
            .map_err(|error| SchemaLoadError::source(&error))?
            != "PostgreSQL"
        {
            return Err(invalid("schema initialization requires PostgreSQL"));
        }
        let metadata = declarations
            .selected_metadata(request.datasource())
            .map_err(|error| SchemaLoadError::source(&error))?;
        let dialect: SqlDialect = CoreDialect::from(request.engine_dialect()).into();
        let version = Self::selected_version(request)?;
        let extensions = RequestedExtensions(request.extensions()).encoded()?;
        if metadata.sql_dialect() != dialect
            || metadata
                .engine_version()
                .as_ref()
                .is_some_and(|value| value != &version)
            || metadata
                .search_path()
                .as_ref()
                .is_some_and(|value| value != request.search_path())
            || metadata
                .postgres_extensions()
                .as_ref()
                .is_some_and(|value| value.to_string() != extensions.as_str())
        {
            return Err(invalid(
                "request schema settings conflict with selected datasource metadata",
            ));
        }
        let mut builder = SqlFunArgs::builder(&HashMap::new())
            .map_err(|error| SchemaLoadError::source(&error))?;
        builder
            .sql_fun_home(home)
            .metadata_file(metadata_path)
            .sql_dialect(dialect)
            .postgres_version(version)
            .postgres_search_path(request.search_path().clone())
            .schema_file(request.schema_dump());
        builder
            .postgres_extensions(extensions)
            .map_err(|error| SchemaLoadError::source(&error))?;
        let args = builder
            .build()
            .map_err(|error| SchemaLoadError::source(&error))?;
        let home = args
            .sql_fun_home()
            .map_err(|error| SchemaLoadError::source(&error))?;
        let mut selected_extensions = args.postgres_extensions(&metadata);
        let files = if request.extensions().is_empty() {
            Vec::new()
        } else {
            let directory = args
                .postgres_extensions_dir(&metadata)
                .map_err(|error| SchemaLoadError::source(&error))?;
            selected_extensions
                .sql_files(directory)
                .map_err(|error| SchemaLoadError::source(&error))?
                .to_vec()
        };
        let selection = SchemaSelection::new(
            request.datasource(),
            &args,
            &metadata,
            &files,
            request.schema_dump(),
            request.expected_hash(),
        )?;
        Ok(Self { home, selection })
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

struct RequestedExtensions<'request>(&'request [::sql_fun_server_api::ExtensionSpec]);

impl RequestedExtensions<'_> {
    fn encoded(&self) -> Result<String, crate::SchemaLoadError> {
        let mut encoded = Vec::new();
        for extension in self.0 {
            if extension.extension_name().contains([',', '@'])
                || extension.extension_version().contains([',', '@'])
            {
                return Err(crate::SchemaLoadError::source(
                    &"extension name/version cannot contain CSV separators",
                ));
            }
            let version =
                ::sql_fun_core::ExtensionVersion::installation(extension.extension_version())
                    .map_err(|error| crate::SchemaLoadError::source(&error))?;
            let pair = ::sql_fun_core::ExtensionNameVersionPair::try_new(
                extension.extension_name(),
                &Some(version),
            )
            .map_err(|error| crate::SchemaLoadError::source(&error))?;
            encoded.push(pair.to_string());
        }
        Ok(encoded.join(","))
    }
}
