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
