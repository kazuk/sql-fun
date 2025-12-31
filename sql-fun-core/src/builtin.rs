use std::path::PathBuf;

use crate::{SqlFunArgs, SqlFunArgsError, SqlFunMetadata};

/// builtin item data path resolver
pub struct PostgresBuiltinPaths {
    builtin_base_path: PathBuf,
}

impl PostgresBuiltinPaths {
    /// create instance
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
            SqlFunArgsError::file_path_not_found(&path)?
        }
        Ok(path)
    }

    /// returns type data file path
    pub fn types(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("pg_type.json"))
    }

    /// returns type data file path
    pub fn functions(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("functions.json"))
    }

    /// function details for table functions
    pub fn function_details(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("func_detail.json"))
    }

    /// binary operators
    pub fn binary_operators(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("binary_op.json"))
    }

    /// left unary operators
    pub fn left_unary_operators(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("left_unary_op.json"))
    }

    /// right unary operators
    pub fn right_unary_operators(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("right_unary_op.json"))
    }

    /// casts
    pub fn casts(&self) -> Result<PathBuf, SqlFunArgsError> {
        Self::path_exists_and_file(self.builtin_base_path.join("casts.json"))
    }
}
