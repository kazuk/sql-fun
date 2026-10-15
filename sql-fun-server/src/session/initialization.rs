use crate::{Api, InitializedSchema, SchemaLoadError, SchemaSelection};
use std::path::PathBuf;

use ::sql_fun_core::SqlFunArgs;
use ::sql_fun_server_api::{SqlFunServerApiError, payloads::InitializeSchemaArgs};

/// Validated schema selection using configuration restored by core.
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

    fn resolve(request: &InitializeSchemaArgs) -> Result<Self, SchemaLoadError> {
        if request.datasource().is_empty() || request.schema_dump().as_os_str().is_empty() {
            return Err(SchemaLoadError::source(
                &"explicit datasource and schema_dump are required",
            ));
        }
        let args = SqlFunArgs::from_environments(request.environments())
            .map_err(|error| SchemaLoadError::source(&error))?;
        let metadata = args
            .selected_metadata(request.datasource())
            .map_err(|error| SchemaLoadError::source(&error))?;
        let home = args
            .sql_fun_home()
            .map_err(|error| SchemaLoadError::source(&error))?;
        let directory = args
            .postgres_extensions_dir(&metadata)
            .map_err(|error| SchemaLoadError::source(&error))?;
        let files = args
            .postgres_extensions(&metadata)
            .sql_files(directory)
            .map_err(|error| SchemaLoadError::source(&error))?;
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
