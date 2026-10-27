//! Exercise the script-owned comparator with real package contents and tools.

#[test]
fn package_api_contracts() {
    run_contracts("test_compare.py");
}

#[test]
fn feature_coverage_contracts() {
    run_contracts("test_lifecycle.py");
}

fn run_contracts(script: &str) {
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
