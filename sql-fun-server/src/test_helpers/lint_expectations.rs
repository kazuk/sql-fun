//! Synthetic catalog and explicit project lint configuration for reporting contracts.
use crate::{
    json_rpc::{JsonRpcError, JsonRpcRequest},
    schema_cache::test_helpers::SchemaProject,
};
use ::serde_json::{Value, json};

pub(crate) struct LintProject {
    project: SchemaProject,
    request: Value,
}
impl LintProject {
    pub(crate) fn new(declarations: &str) -> Self {
        let project = SchemaProject::new();
        let metadata_path = project.home().join("Cargo.toml");
        let metadata = std::fs::read_to_string(&metadata_path).unwrap().replace(
            "[package.metadata.database.products]",
            "[package.metadata.database.products]\nlint_configuration = 'lint.toml'",
        );
        std::fs::write(metadata_path, metadata).unwrap();
        let request = project.rpc_request("CREATE TABLE retained (id int4, value int4);");
        let fixture = Self { project, request };
        fixture.configure(declarations);
        fixture
    }
    pub(crate) fn omit_configuration(&self) {
        let path = self.project.home().join("Cargo.toml");
        let metadata = std::fs::read_to_string(&path)
            .unwrap()
            .replace("lint_configuration = 'lint.toml'\n", "");
        std::fs::write(path, metadata).unwrap();
    }

    pub(crate) fn configure(&self, declarations: &str) {
        std::fs::write(self.lint_path(), declarations).unwrap();
    }
    pub(crate) fn lint_path(&self) -> std::path::PathBuf {
        self.project.home().join("lint.toml")
    }
    pub(crate) fn request(&self) -> &Value {
        &self.request
    }
}

pub(crate) fn rpc(method: &str, params: &Value) -> JsonRpcRequest {
    ::serde_json::from_value(json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params}))
        .unwrap()
}

pub(crate) fn rule(name: &str, expect: &str) -> String {
    format!(
        r#"[[lint.deprecated]]
kind = "table"
name = {name}
{expect}
"#
    )
}

/// Keep structured RPC error details visible when a success scenario fails.
pub(crate) fn rpc_failed(error: JsonRpcError) -> Value {
    panic!("RPC failed: {}", ::serde_json::to_value(error).unwrap())
}
