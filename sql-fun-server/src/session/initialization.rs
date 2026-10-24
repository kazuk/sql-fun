use crate::{Api, InitializedSchema, SchemaLoadError, SchemaSelection};
use std::path::PathBuf;

use ::sql_fun_core::SqlFunArgs;
use ::sql_fun_server_api::{SqlFunServerApiError, payloads::InitializeSchemaArgs};

/// Validated schema selection using configuration restored by core.
pub(super) struct Initialization {
    home: PathBuf,
    selection: SchemaSelection,
    rewrite_options: ::sql_fun_core::RewriteOptions,
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
        Ok(Self {
            home,
            selection,
            rewrite_options: *metadata.rewrite_options(),
        })
    }

    pub(super) const fn rewrite_options(&self) -> &::sql_fun_core::RewriteOptions {
        &self.rewrite_options
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
