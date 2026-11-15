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
                .sql_files(directory)
                .map_err(|error| SchemaLoadError::source(&error))?
        } else {
            ::sql_fun_core::IVec::default()
        };
        let selection = SchemaSelection::for_level(
            request.datasource(),
            &args,
            &metadata,
            &files,
            request.schema_dump().as_deref(),
            request.expected_hash().as_deref(),
            level,
        )?;
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
