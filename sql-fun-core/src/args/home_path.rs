use std::path::{Path, PathBuf};

use super::SqlFunArgsError;

/// Resolve an explicitly supplied sql-fun home path to an absolute path.
///
/// Absolute paths are unchanged. `~/` paths use the user's home directory;
/// other relative paths use the current working directory.
///
/// # Errors
/// Returns an error for an empty path or when a required base directory is unavailable.
pub fn resolve_sql_fun_home_path(path: &Path) -> Result<PathBuf, SqlFunArgsError> {
    resolve_with(
        path,
        || std::env::current_dir().map_err(SqlFunArgsError::current_directory_unavailable),
        std::env::home_dir,
    )
}

fn resolve_with(
    path: &Path,
    current_dir: impl FnOnce() -> Result<PathBuf, SqlFunArgsError>,
    home_dir: impl FnOnce() -> Option<PathBuf>,
) -> Result<PathBuf, SqlFunArgsError> {
    if path.as_os_str().is_empty() {
        return SqlFunArgsError::empty_home_path();
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if let Ok(rest) = path.strip_prefix("~") {
        let home = home_dir().filter(|home| home.is_absolute());
        let Some(home) = home else {
            return SqlFunArgsError::user_home_not_defined();
        };
        return Ok(std::path::absolute(home.join(rest))?);
    }
    Ok(std::path::absolute(current_dir()?.join(path))?)
}

#[cfg(test)]
mod test_resolve_sql_fun_home_path {
    use super::{resolve_sql_fun_home_path, resolve_with};
    use crate::SqlFunArgsBuilder;
    use std::path::Path;

    #[rstest::rstest]
    #[case::absolute("/tmp/home", Path::new("/tmp/home"))]
    #[case::relative("./home", Path::new("home"))]
    #[case::parent("../home", Path::new("home"))]
    fn resolves_absolute_or_working_directory_paths(#[case] input: &str, #[case] suffix: &Path) {
        let resolved = resolve_sql_fun_home_path(Path::new(input)).unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with(suffix));
    }

    #[test]
    fn expands_home_relative_path() {
        let home = std::env::home_dir().unwrap();
        assert_eq!(
            resolve_sql_fun_home_path(Path::new("~/work")).unwrap(),
            home.join("work")
        );
    }

    #[test]
    fn rejects_empty_path() {
        assert!(resolve_sql_fun_home_path(Path::new("")).is_err());
    }

    #[test]
    fn unavailable_home_rejects_home_relative_path() {
        assert!(resolve_with(Path::new("~/work"), || unreachable!(), || None).is_err());
    }

    #[test]
    fn unavailable_working_directory_rejects_relative_path() {
        let unavailable = std::io::Error::new(std::io::ErrorKind::NotFound, "cwd unavailable");
        let error = resolve_with(
            Path::new("./work"),
            || {
                Err(super::SqlFunArgsError::current_directory_unavailable(
                    unavailable,
                ))
            },
            || unreachable!(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("current working directory unavailable")
        );
    }

    #[rstest::rstest]
    #[case::relative("./work", std::path::absolute("./work").unwrap())]
    #[case::home_relative("~/work", std::env::home_dir().unwrap().join("work"))]
    #[case::absolute("/tmp/work", Path::new("/tmp/work").to_path_buf())]
    fn common_option_and_environment_values_share_resolution(
        #[case] input: &str,
        #[case] expected: std::path::PathBuf,
    ) {
        let args = SqlFunArgsBuilder::default()
            .sql_fun_home(input)
            .build()
            .unwrap();
        assert_eq!(args.initialization_home_candidate(), Some(Path::new(input)));
        assert_eq!(args.initialization_home().unwrap(), expected);
    }
}
