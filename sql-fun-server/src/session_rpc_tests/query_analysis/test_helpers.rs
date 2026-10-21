use crate::schema_cache::test_helpers::SchemaProject;

/// A small int4 catalog with the comparison used by the RPC scenarios.
pub(super) fn query_project() -> SchemaProject {
    let project = SchemaProject::new();
    let builtin = project.home().join("postgres/17");
    std::fs::write(
        builtin.join("pg_type.json"),
        r#"[{"name":"int4"},{"name":"bool"},{"name":"text"}]"#,
    )
    .unwrap();
    std::fs::write(
        builtin.join("binary_op.json"),
        r#"{
        "+":[{"left":"int4","right":"int4","result":"int4","strict":true}],
        "=":[{"left":"int4","right":"int4","result":"bool","strict":true}]
    }"#,
    )
    .unwrap();
    project
}

/// An explicitly selected query file in the fixture's own temporary project.
pub(super) fn query_file(project: &SchemaProject, sql: &str) -> std::path::PathBuf {
    let path = project.home().join("selected-query.sql");
    std::fs::write(&path, sql).unwrap();
    path
}
