//! Exercise the script-owned comparator with real package contents and tools.

use crate::test_helpers::run_contracts;

#[test]
fn package_api_contracts() {
    run_contracts("test_compare.py");
}

// Removed feature_coverage_contracts after a CI timeout in
// 5cb85fdfb04141bdbb3f3fe45d914fd0d024de21. Its Python contracts, along with
// source membership checks, run in the standard version-materialization step.

#[test]
fn release_intent_contracts() {
    run_contracts("test_intent.py");
}

mod test_helpers {
    pub(super) fn run_contracts(script: &str) {
        // nextest remaps these runtime paths when running a CI build archive.
        let manifest_dir =
            std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo/nextest package directory");
        let root = std::path::Path::new(&manifest_dir)
            .parent()
            .expect("workspace root");
        let wrapper = std::env::var_os("NEXTEST_BIN_EXE_xbuild")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_xbuild").into());
        // uv is already part of contributor setup for graphify. It provisions the
        // test interpreter without adding a manual Python installation step.
        let status = std::process::Command::new("uv")
            .args(["run", "--no-project", "--no-config", "--python", "3.12"])
            .arg(root.join("tests/package-api").join(script))
            .env("PACKAGE_API_DIFF_WRAPPER", wrapper)
            .current_dir(root)
            .status()
            .expect("uv comparator contracts");
        assert!(status.success(), "package API comparator contracts failed");
    }
}
