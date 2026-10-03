use std::{
    collections::{HashMap, hash_map::Entry},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use sql_fun_core::{
    CteCatalog, CteCatalogReference, CteName, PlaceholderedSql, ResolvedCteCatalog,
};

use crate::{
    error::{ConfigurationKind, SqlFunCodegenError},
    rust_syn::{CteReferenceAttr, ItemFunction},
};

pub trait CteCatalogSupport {
    fn cte_references(
        item_fn: &ItemFunction,
        metadata: &sql_fun_core::SqlFunMetadata,
    ) -> Result<CteReferenceCollection, SqlFunCodegenError> {
        let cte_references = CteReferenceAttr::from_item_fn(item_fn)?;
        if !cte_references.is_empty()
            && let Some(cte_catalog_path) = metadata.cte_catalog()?
        {
            CteCatalogCache::get_or_fill(
                cte_catalog_path,
                |catalog| -> Result<(), SqlFunCodegenError> {
                    let mut resolved = CteReferenceResolveResultCollection::new_empty();
                    for cte_reference in &cte_references {
                        let name = cte_reference.cte_name();
                        if let Some(cte) = catalog.get_cte(name) {
                            if cte.has_error() {
                                resolved.push_has_error(name);
                            } else {
                                resolved.push_success(name);
                            }
                        } else {
                            resolved.push_missing(name);
                        }
                    }
                    Ok(())
                },
            )?;
        }
        Ok(CteReferenceCollection::from(cte_references))
    }
}

/// cache a cte catalog
pub struct CteCatalogCache<'a> {
    entries: HashMap<PathBuf, CteCatalog<'a>>,
}

static CTE_CATALOGS: LazyLock<Mutex<CteCatalogCache<'static>>> =
    LazyLock::new(|| Mutex::new(CteCatalogCache::default()));

impl CteCatalogCache<'_> {
    fn default() -> Self {
        Self {
            entries: HashMap::default(),
        }
    }

    /// get or fill entry
    pub fn get_or_fill<P, F, T, E>(path: P, handler: F) -> Result<T, SqlFunCodegenError>
    where
        P: AsRef<Path>,
        F: Fn(&CteCatalog<'_>) -> Result<T, E>,
        E: Into<SqlFunCodegenError>,
    {
        let path = path.as_ref();
        let mut catalogs = CTE_CATALOGS
            .lock()
            .map_err(|_| SqlFunCodegenError::CteCatalogPoisoned())?;
        match catalogs.entries.entry(path.to_path_buf()) {
            Entry::Occupied(o) => Ok(handler(o.get()).map_err(std::convert::Into::into)?),
            Entry::Vacant(v) => {
                let catalog = CteCatalog::load(path)?;

                let catalog = v.insert(catalog);
                Ok(handler(catalog).map_err(std::convert::Into::into)?)
            }
        }
    }
}

/// CTE catalog resolve result
pub enum CteReferenceResolveResult {
    /// success fully resolved
    #[expect(
        dead_code,
        reason = "Resolved names are stored, but resolution-result consumers are not yet implemented."
    )]
    Success(CteName),
    /// resolved but definition error
    #[expect(
        dead_code,
        reason = "Resolved names are stored, but resolution-result consumers are not yet implemented."
    )]
    HasError(CteName),
    /// undefined CTE name
    #[expect(
        dead_code,
        reason = "Resolved names are stored, but resolution-result consumers are not yet implemented."
    )]
    Missing(CteName),
}

/// collection for [`CteReferenceResolveResult`]
pub struct CteReferenceResolveResultCollection(Vec<CteReferenceResolveResult>);

impl CteReferenceResolveResultCollection {
    /// create new empty collection
    pub fn new_empty() -> Self {
        Self(Vec::default())
    }

    /// push [`CteReferenceResolveResult::Success`]
    pub fn push_success(&mut self, cte_name: &CteName) {
        self.0
            .push(CteReferenceResolveResult::Success(cte_name.clone()));
    }

    /// push [`CteReferenceResolveResult::HasError`]
    pub fn push_has_error(&mut self, cte_name: &CteName) {
        self.0
            .push(CteReferenceResolveResult::HasError(cte_name.clone()));
    }

    /// push [`CteReferenceResolveResult::Missing`]
    pub fn push_missing(&mut self, cte_name: &CteName) {
        self.0
            .push(CteReferenceResolveResult::Missing(cte_name.clone()));
    }
}

#[derive(Clone, Debug)]
pub struct CteReferenceCollection(Vec<CteReferenceAttr>);

impl From<Vec<CteReferenceAttr>> for CteReferenceCollection {
    fn from(value: Vec<CteReferenceAttr>) -> Self {
        Self(value)
    }
}

impl CteReferenceCollection {
    #[tracing::instrument(skip(metadata, query))]
    pub fn resolve(
        &self,
        metadata: &sql_fun_core::SqlFunMetadata,
        query: &PlaceholderedSql,
    ) -> Result<ResolvedCteCatalog, SqlFunCodegenError> {
        if self.0.is_empty() {
            return Ok(ResolvedCteCatalog::from(query));
        }
        let Some(cte_catalog_path) = metadata.cte_catalog()? else {
            return SqlFunCodegenError::configuration_missing_error(
                ConfigurationKind::MetadataCteCatalog,
            );
        };

        let resolved = CteCatalogCache::get_or_fill(
            cte_catalog_path,
            |catalog| -> Result<ResolvedCteCatalog, SqlFunCodegenError> {
                let cte_refs: Vec<CteCatalogReference> = self
                    .0
                    .iter()
                    .map(|item| item.as_cte_reference().clone())
                    .collect();

                let resolved = catalog.resolve(query, &cte_refs)?;
                Ok(resolved)
            },
        )?;
        Ok(resolved)
    }
}
