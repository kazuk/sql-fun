use std::{
    collections::{HashMap, hash_map::Entry},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};

use sql_fun_core::{Hasher, SqlFunArgs, SqlFunMetadata, lockfile::Lockfile};

use crate::error::SqlFunCodegenError;

#[derive(Default)]
pub struct LockfileProvider {
    lockfiles: HashMap<PathBuf, Arc<Lockfile>>,
}

static LOCKFILE_PROVIDER: LazyLock<Mutex<LockfileProvider>> =
    LazyLock::new(|| Mutex::new(LockfileProvider::default()));

impl LockfileProvider {
    fn load(
        &mut self,
        args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
    ) -> Result<Arc<Lockfile>, SqlFunCodegenError> {
        let lockfile_path = metadata.lockfile_path(args)?;
        match self.lockfiles.entry(lockfile_path.clone()) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(e) => {
                let schema_dump_path = metadata.schema_dump_path(args)?;
                if !schema_dump_path.exists() {
                    SqlFunCodegenError::raise_schema_dump_not_exist(schema_dump_path)?;
                    SqlFunCodegenError::never()
                }
                let schema_dump_hash_code = Hasher::hash_file(schema_dump_path)?;

                let lockfile =
                    Arc::new(Lockfile::from_file(&lockfile_path, &schema_dump_hash_code)?);
                e.insert(lockfile.clone());
                Ok(lockfile)
            }
        }
    }

    #[tracing::instrument(err, skip(args, metadata))]
    pub fn load_from(
        args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
    ) -> Result<Arc<Lockfile>, SqlFunCodegenError> {
        let mut provider = LOCKFILE_PROVIDER
            .lock()
            .map_err(|_e| SqlFunCodegenError::unexpected_lockfile_mutex_poisoned())?;
        provider.load(args, metadata)
    }

    #[tracing::instrument(err, skip(args))]
    pub fn load_datasource(
        args: &SqlFunArgs,
        datasource_name: &str,
    ) -> Result<Arc<Lockfile>, SqlFunCodegenError> {
        let metadata_collection = args.load_metadata()?;
        let Some(metadata) = metadata_collection.get(datasource_name) else {
            return SqlFunCodegenError::datasource_not_found(datasource_name);
        };
        let lockfile = Self::load_from(args, metadata)?;
        Ok(lockfile)
    }
}

#[cfg(test)]
mod test_lockfile_provider {
    use crate::{lockfile::LockfileProvider, test_helpers::LockfileProject};

    #[rstest::rstest]
    #[case::explicit_without_default(true, false)]
    #[case::explicit_overrides_competing_default(true, true)]
    #[case::derived_default(false, false)]
    fn loads_lockfile_for_selected_schema(#[case] explicit: bool, #[case] competing: bool) {
        let project = LockfileProject::new(explicit, competing);
        let mut provider = LockfileProvider::default();
        assert!(provider.load(project.args(), project.metadata()).is_ok());
    }

    #[test]
    fn missing_explicit_schema_does_not_use_existing_default() {
        let project = LockfileProject::new(true, true);
        project.remove_selected_schema();
        let mut provider = LockfileProvider::default();
        let error = provider
            .load(project.args(), project.metadata())
            .unwrap_err();
        assert!(error.to_string().contains("selected.sql"));
    }

    #[test]
    fn rejects_changed_explicit_schema() {
        let project = LockfileProject::new(true, true);
        project.change_selected_schema();
        let mut provider = LockfileProvider::default();
        assert!(provider.load(project.args(), project.metadata()).is_err());
    }
}
