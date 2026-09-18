use std::{
    convert::Infallible,
    path::{Path, PathBuf},
};

use crate::{
    CatalogFileIndex, CteCatalog, CteCatalogReference, CteName, CteParameterRename,
    placehlodered_sql::ParameterName,
};

/// Errors for [`super::CteCatalog`]
#[derive(thiserror::Error, Debug)]
pub enum CteCatalogError {
    /// [`std::io::Error`]
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    /// [`walkdir::Error`]
    #[error(transparent)]
    WalkDirError(#[from] walkdir::Error),

    /// [`serde_json::Error`]
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),

    /// [`toml::de::Error`]
    #[error(transparent)]
    TomlError(#[from] toml::de::Error),

    /// [`yaml_serde::Error`]
    #[error(transparent)]
    YamlError(#[from] serde_yaml_ng::Error),

    /// specified path not exist
    #[error("Specified path not exist {0}")]
    PathNotExists(PathBuf),

    /// path not exist
    #[error("Specified path not directory {0}")]
    PathIsNotDirectory(PathBuf),

    /// unsupported file format
    #[error("Unsupported file format {0} : {1}")]
    UnsupportedFileFormat(String, PathBuf),

    /// file is not CTE Catalog
    #[error("File is not CTE Catalog {0}")]
    FileIsNotCteCatalog(PathBuf),

    /// Named cte not found
    #[error("{0} is not defined in CTE catalog.")]
    CteNotFound(CteName),

    /// Parameter collection not exist
    #[error("[Unexpected internal error / BUG] Parameter collection not exist.")]
    ParameterCollectionNotExist(),

    /// Validation error
    #[error("CTE catalog validation failed")]
    Validation(Vec<CteCatalogValidationError>),
}

impl CteCatalogError {
    pub(super) fn raise_path_not_exist<P: AsRef<Path>>(path: P) -> Result<Infallible, Self> {
        Err(Self::PathNotExists(path.as_ref().to_path_buf()))
    }

    pub(super) fn raise_path_is_not_directory<P: AsRef<Path>>(path: P) -> Result<Infallible, Self> {
        Err(Self::PathIsNotDirectory(path.as_ref().to_path_buf()))
    }

    pub(super) fn raise_unsupported_file_format<P: AsRef<Path>>(
        path: P,
    ) -> Result<Infallible, Self> {
        let extension = path.as_ref().extension().map_or_else(
            || String::from("None"),
            |os_str| os_str.to_string_lossy().to_string(),
        );

        Err(Self::UnsupportedFileFormat(
            extension,
            path.as_ref().to_path_buf(),
        ))
    }

    pub(super) fn raise_file_is_not_cte_catalog<P: AsRef<Path>>(
        path: P,
    ) -> Result<Infallible, Self> {
        Err(Self::FileIsNotCteCatalog(path.as_ref().to_path_buf()))
    }

    pub(super) fn raise_cte_not_found(cte_name: &CteName) -> Result<Infallible, Self> {
        Err(Self::CteNotFound(cte_name.clone()))
    }

    pub(super) fn unexpected_parameter_collection_not_exist() -> Result<Infallible, Self> {
        Err(Self::ParameterCollectionNotExist())
    }

    pub(super) fn validation_error(
        validation_errors: Vec<CteCatalogValidationError>,
    ) -> Result<Infallible, Self> {
        Err(Self::Validation(validation_errors))
    }

    pub(super) fn never() -> ! {
        unreachable!()
    }
}

/// CTE catalog semantics error
#[derive(Debug)]
pub enum CteCatalogCheckError<'a> {
    /// Existing CTE query as same name
    DuplicateDefinition(CatalogFileIndex<'a>),
    /// referencing entry not found
    UndefinedEntryReference(CteName),
    /// CTE entry not used
    UnusedCteEntry,
    /// fragment embedding syntax detected in query
    FragmentEmbeddingNotSupported,
    /// cyclic dependency
    CyclicDependency(CteName, CteCatalogReference),
    /// parameter not found
    ParameterNotFound(CteName, ParameterName),
}

impl<'a> CteCatalogCheckError<'a> {
    /// multiple CTE has a same name
    #[must_use]
    pub(super) fn duplicate_definition(other_definition_in: CatalogFileIndex<'a>) -> Self {
        Self::DuplicateDefinition(other_definition_in)
    }

    /// specified CTE name not defined
    #[must_use]
    pub(super) fn undefined_entry_reference(cte_name: &CteName) -> Self {
        Self::UndefinedEntryReference(cte_name.clone())
    }

    /// CTE defined but not used
    #[must_use]
    pub(super) fn unused_cte_entry() -> Self {
        Self::UnusedCteEntry
    }

    /// fragment syntax used in CTE definition
    #[must_use]
    pub(super) fn fragment_embedding_not_supported() -> Self {
        Self::FragmentEmbeddingNotSupported
    }

    #[must_use]
    pub(super) fn cyclic_dependency(cte_name: &CteName, dep: &CteCatalogReference) -> Self {
        Self::CyclicDependency(cte_name.clone(), dep.clone())
    }

    #[must_use]
    pub(super) fn parameter_not_found(cte_name: &CteName, parameter_name: &ParameterName) -> Self {
        Self::ParameterNotFound(cte_name.clone(), parameter_name.clone())
    }

    #[must_use]
    pub(super) fn parameter_not_found_for_rename(rename_def: &CteParameterRename) -> Self {
        Self::parameter_not_found(rename_def.cte_name(), rename_def.parameter_name())
    }
}

/// CTE catalog validation error
///
/// # Note
///
/// non-lifetime bound version [`CteCatalogCheckError`]
#[derive(Debug)]
pub enum CteCatalogValidationError {
    /// Duplicate definition
    DuplicateDefinition(CteName, PathBuf),

    /// reference entry not found
    UndefinedEntryReference(CteName, CteName),

    /// CTE entry not used
    UnusedCteEntry(CteName),

    /// fragment embedding not supported
    FragmentEmbeddingNotSupported(CteName),

    /// Cyclic dependency detected
    CyclicDependency(CteName, CteName),

    /// parameter not found
    ParameterNotFound(CteName, CteName, ParameterName),
}

impl CteCatalogValidationError {
    fn from_check_error<'a>(
        cte_catalog: &CteCatalog<'a>,
        cte_name: &CteName,
        check_error: &CteCatalogCheckError<'a>,
    ) -> Self {
        match check_error {
            CteCatalogCheckError::DuplicateDefinition(file_index) => {
                let file_path = cte_catalog.get_file_path(*file_index).clone();
                Self::DuplicateDefinition(cte_name.clone(), file_path)
            }
            CteCatalogCheckError::UndefinedEntryReference(ref_name) => {
                Self::UndefinedEntryReference(cte_name.clone(), ref_name.clone())
            }
            CteCatalogCheckError::UnusedCteEntry => Self::UnusedCteEntry(cte_name.clone()),
            CteCatalogCheckError::FragmentEmbeddingNotSupported => {
                Self::FragmentEmbeddingNotSupported(cte_name.clone())
            }
            CteCatalogCheckError::CyclicDependency(ref_cte, _) => {
                Self::CyclicDependency(cte_name.clone(), ref_cte.clone())
            }
            CteCatalogCheckError::ParameterNotFound(ref_cte, param) => {
                Self::ParameterNotFound(cte_name.clone(), ref_cte.clone(), param.clone())
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct CteCatalogCheckErrorCollection<'a>(Vec<CteCatalogCheckError<'a>>);

impl<'a> CteCatalogCheckErrorCollection<'a> {
    pub fn push_parameter_not_found(&mut self, rename_def: &CteParameterRename) {
        self.0
            .push(CteCatalogCheckError::parameter_not_found_for_rename(
                rename_def,
            ));
    }

    pub fn push_undefined_entry_reference(&mut self, cte_name: &CteName) {
        self.0
            .push(CteCatalogCheckError::undefined_entry_reference(cte_name));
    }

    pub fn push_fragment_embedding_not_supported(&mut self) {
        self.0
            .push(CteCatalogCheckError::fragment_embedding_not_supported());
    }

    pub fn push_duplicate_definition_error(&mut self, file_index: CatalogFileIndex<'a>) {
        self.0
            .push(CteCatalogCheckError::duplicate_definition(file_index));
    }

    pub fn push_unused_cte_entry(&mut self) {
        self.0.push(CteCatalogCheckError::unused_cte_entry());
    }

    pub fn push_cyclic_dependency(&mut self, cte_name: &CteName, dep: &CteCatalogReference) {
        self.0
            .push(CteCatalogCheckError::cyclic_dependency(cte_name, dep));
    }

    pub fn merge(&mut self, mut source: Self) {
        self.0.append(&mut source.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn validation_errors(
        &self,
        cte_catalog: &CteCatalog<'a>,
        cte_name: &CteName,
    ) -> Vec<CteCatalogValidationError> {
        self.0
            .iter()
            .map(|item| CteCatalogValidationError::from_check_error(cte_catalog, cte_name, item))
            .collect()
    }

    #[cfg(test)]
    pub fn has_error(&self, predicate: impl Fn(&CteCatalogCheckError<'_>) -> bool) -> bool {
        self.0.iter().any(predicate)
    }
}
