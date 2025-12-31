use std::path::PathBuf;

use crate::SqlFunArgsError;

impl super::SqlFunArgs {
    /// gets metadata format value in args
    pub fn metadata_format(&self) -> &str {
        self.metadata_args.metadata_format()
    }

    /// return raw value for `--metadata-file`
    pub fn metadata_file_raw(&self) -> Result<&str, SqlFunArgsError> {
        self.metadata_args.metadata_file_raw()
    }

    /// search metadata file with same-owner policy
    pub fn metadata_file(&self) -> Result<PathBuf, SqlFunArgsError> {
        self.metadata_args.metadata_file()
    }

    /// get schema file
    pub fn schema_file(&self, metadata_file: &PathBuf) -> Result<PathBuf, SqlFunArgsError> {
        self.metadata_args.schema_file(metadata_file)
    }
}

#[cfg(test)]
mod test_metadata {
    use std::str::FromStr;

    use clap::{Parser, builder::OsStr};
    use testresult::TestResult;

    use crate::{SqlDialect, SqlFunArgs, SqlFunMetadata, metadata::EngineVersion};

    #[test]
    pub fn test_metadata_format() -> TestResult {
        let args = SqlFunArgs::try_parse_from(vec![
            "sqlfun",
            "--sql-fun-home",
            "../sql-fun-home",
            "subcmd",
        ])?;
        let metadata_format = args.metadata_format();
        assert_eq!(metadata_format, "TOML");
        Ok(())
    }

    #[test]
    pub fn test_metadata_file_raw() -> TestResult {
        let args = SqlFunArgs::try_parse_from(vec![
            "sqlfun",
            "--sql-fun-home",
            "../sql-fun-home",
            "subcmd",
        ])?;
        let metadata_file_raw = args.metadata_file_raw()?;
        assert_eq!(metadata_file_raw, "Cargo.toml");
        Ok(())
    }

    #[test]
    pub fn test_metadata_file() -> TestResult {
        let args = SqlFunArgs::try_parse_from(vec![
            "sqlfun",
            "--sql-fun-home",
            "../sql-fun-home",
            "subcmd",
        ])?;
        let metadata_file = args.metadata_file()?;
        assert_eq!(
            metadata_file.parent().unwrap().file_stem().unwrap(),
            OsStr::from("sql-fun-core")
        );
        Ok(())
    }

    #[test]
    pub fn test_metadata_file_load() -> TestResult {
        let args = SqlFunArgs::try_parse_from(vec![
            "sqlfun",
            "--sql-fun-home",
            "../sql-fun-home",
            "--metadata-file",
            "test-assets/metadata.toml",
            "subcmd",
        ])?;
        let metadata_file = args.metadata_file()?;
        let metadata = SqlFunMetadata::load_from(metadata_file)?;
        let sql_dialect = metadata.sql_dialect();
        assert_eq!(sql_dialect, SqlDialect::PostgreSQL);
        let engine_version = metadata.engine_version();
        assert_eq!(engine_version, &Some(EngineVersion::from_str("17")?));

        Ok(())
    }
}
