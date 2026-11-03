pub(super) mod request_peer;

#[allow(
    dead_code,
    reason = "shared API fixtures include helpers for other integration targets"
)]
#[path = "../api_helpers.rs"]
pub(super) mod api;

pub(crate) use self::api::cli_executable;

use std::path::{Path, PathBuf};

/// Owns only suite input and output files; does not parse a catalog or start services.
pub(super) struct Suite {
    directory: ::tempfile::TempDir,
    input: PathBuf,
    expected: PathBuf,
}
impl Suite {
    pub(super) fn new(
        queries: &[&str],
        expectations: &::serde_json::Value,
    ) -> ::testresult::TestResult<Self> {
        let directory = ::tempfile::tempdir()?;
        let input = directory.path().join("suite.json");
        let expected = directory.path().join("expected.json");
        std::fs::write(
            &input,
            ::serde_json::to_vec(&::serde_json::json!({"queries":queries}))?,
        )?;
        std::fs::write(&expected, ::serde_json::to_vec(expectations)?)?;
        Ok(Self {
            directory,
            input,
            expected,
        })
    }
    pub(super) fn selection(&self) -> String {
        format!("{}#$.queries", self.input.display())
    }
    pub(super) fn expected(&self) -> &Path {
        &self.expected
    }
    pub(super) fn report(&self) -> PathBuf {
        self.directory.path().join("reports/junit.xml")
    }
    pub(super) fn report_text(&self) -> ::testresult::TestResult<String> {
        Ok(std::fs::read_to_string(self.report())?)
    }
}

impl Suite {
    pub(super) fn paged_columns() -> ::testresult::TestResult<Self> {
        let query = format!(
            "SELECT {} FROM events",
            (0..1025)
                .map(|index| format!("id AS c{index}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let columns: Vec<_> = (0..1025).map(|index| ::serde_json::json!({"name":format!("c{index}"), "type":{"schema_name":"pg_catalog","local_name":"int4","is_array":false},"nullability":"not_null"})).collect();
        Self::new(
            &[&query],
            &::serde_json::json!([{"diagnostics":[],"columns":columns}]),
        )
    }
}
