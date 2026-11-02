/// Nextest remaps the package binary when executing a relocated build archive.
/// Direct local invocations retain the contributor-provided executable input.
pub(crate) fn cli_executable() -> std::ffi::OsString {
    if let Some(binary) = std::env::var_os("NEXTEST_BIN_EXE_sql_fun") {
        return binary;
    }
    std::env::var_os("SQL_FUN_BIN").expect("nextest CLI binary or SQL_FUN_BIN must be set")
}

// Shared integration suites compile only the fixture operations they use.
#[allow(dead_code)]
// Also loaded as api_helpers::executable via #[path], so resolve from this file.
#[path = "test_helpers/initialization.rs"]
pub(crate) mod initialization;
