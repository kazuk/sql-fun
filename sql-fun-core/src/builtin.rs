use std::path::PathBuf;

use crate::{SqlFunArgs, SqlFunArgsError, SqlFunMetadata};

/// builtin item data path resolver
pub struct PostgresBuiltinPaths {
    builtin_base_path: PathBuf,
}

impl PostgresBuiltinPaths {
    /// create instance
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when builtin directories cannot be resolved.
    pub fn try_from_args(
        args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
    ) -> Result<Self, SqlFunArgsError> {
        let base_path = args.builtin_info_dir(metadata)?;
        Ok(Self {
            builtin_base_path: base_path,
        })
    }

    fn path_exists_and_file(path: PathBuf) -> Result<PathBuf, SqlFunArgsError> {
        if !path.exists() || !path.is_file() {
            SqlFunArgsError::file_path_not_found(&path)?;
        }
        Ok(path)
    }

    /// returns type data file path
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the expected file is missing.
    pub fn types(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("pg_type.json"))
    }

    /// returns type data file path
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the expected file is missing.
    pub fn functions(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("functions.json"))
    }

    /// function details for table functions
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the expected file is missing.
    pub fn function_details(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("func_detail.json"))
    }

    /// binary operators
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the expected file is missing.
    pub fn binary_operators(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("binary_op.json"))
    }

    /// left unary operators
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the expected file is missing.
    pub fn left_unary_operators(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("left_unary_op.json"))
    }

    /// right unary operators
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the expected file is missing.
    pub fn right_unary_operators(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("right_unary_op.json"))
    }

    /// casts
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the expected file is missing.
    pub fn casts(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("casts.json"))
    }
}
