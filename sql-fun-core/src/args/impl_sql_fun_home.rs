use crate::SqlFunArgsError;
use std::path::PathBuf;

use crate::args::SqlFunArgs;

impl SqlFunArgs {
    /// get home dir path
    ///
    /// If `--sql-fun-home` or `SQL_FUN_HOME` env present : configured value
    /// Else returns `${HOME}/.sql-fun`
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the home directory cannot be resolved.
    pub fn sql_fun_home(&self) -> Result<PathBuf, SqlFunArgsError> {
        self.metadata_args.sql_fun_home()
    }
}

#[cfg(test)]
mod test_sql_fun_home {
    use clap::Parser;
    use testresult::TestResult;

    use crate::SqlFunArgs;

    #[test]
    pub fn test_home() -> TestResult {
        let args = SqlFunArgs::try_parse_from(vec!["sqlfun", "--sql-fun-home", "../sql-fun-home"])?;
        let home = args.sql_fun_home()?;

        assert!(home.is_absolute());
        assert!(home.ends_with("sql-fun-home"), "{home:?}");
        Ok(())
    }
}
