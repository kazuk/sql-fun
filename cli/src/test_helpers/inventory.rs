use ::sql_fun_client::{
    Collection,
    payloads::{
        AccessKind, AnalyzeQueryItem, AnalyzeQueryResponse, ColumnAccess, ObjectKind, RelationName,
    },
};

/// Two statements with duplicate and distinct roles, independent of generation state.
pub(crate) fn access_response(eligible: bool) -> AnalyzeQueryResponse {
    let relation = RelationName::new(None, Some("odd.schema"), "a.\"table");
    let accesses = vec![
        ColumnAccess::new(
            AccessKind::update_rows(),
            ObjectKind::table(),
            &relation,
            None,
        ),
        ColumnAccess::new(
            AccessKind::update_target(),
            ObjectKind::table(),
            &relation,
            Some("a.\"column"),
        ),
        ColumnAccess::new(
            AccessKind::update_input(),
            ObjectKind::table(),
            &relation,
            Some("a.\"column"),
        ),
        ColumnAccess::new(
            AccessKind::update_input(),
            ObjectKind::table(),
            &relation,
            Some("a.\"column"),
        ),
    ];
    let second = vec![
        ColumnAccess::new(
            AccessKind::read_rows(),
            ObjectKind::table(),
            &relation,
            None,
        ),
        ColumnAccess::new(
            AccessKind::select_output(),
            ObjectKind::table(),
            &relation,
            Some("a.\"column"),
        ),
    ];
    let mut statements = Vec::new();
    for records in [accesses, second] {
        statements.push(AnalyzeQueryItem::new(
            None,
            None,
            if eligible {
                Some("SQL".to_owned())
            } else {
                None
            },
            eligible,
            Collection::completed(if eligible {
                Vec::new()
            } else {
                let problem =
                    ::sql_fun_core::analysis_problem::AnalysisProblem::parameter_constraint(
                        "unresolved parameter",
                    );
                vec![
                    ::sql_fun_client::payloads::AnalysisProbrem::new(
                        problem.code(),
                        ::sql_fun_client::payloads::DiagnosticSeverity::error(),
                        std::path::Path::new("query"),
                        None,
                    )
                    .with_semantic_problem(&problem),
                ]
            }),
            Collection::completed(Vec::new()),
            Collection::completed(Vec::new()),
            Collection::completed(records),
        ));
    }
    AnalyzeQueryResponse::new(Collection::completed(statements))
}

/// Datasource-owned files, with no real builtin catalog or server required.
pub(crate) struct Project(::tempfile::TempDir);

impl Project {
    pub(crate) fn new() -> Self {
        let directory = ::tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("Cargo.toml"),
            r#"
[package.metadata.database.first]
engine = "PostgreSQL"
dialect = "postgres"
version = "18"
connector = "tokio-postgres"
[package.metadata.database.second]
engine = "PostgreSQL"
dialect = "postgres"
version = "18"
connector = "tokio-postgres"
"#,
        )
        .unwrap();
        std::fs::write(
            directory.path().join("schema.sql"),
            "CREATE TABLE entries(id int4);",
        )
        .unwrap();
        Self(directory)
    }

    pub(crate) fn args(&self) -> ::sql_fun_core::SqlFunArgs {
        ::sql_fun_core::SqlFunArgs::builder(&std::collections::HashMap::new())
            .unwrap()
            .metadata_file(self.0.path().join("Cargo.toml").to_str().unwrap())
            .sql_fun_home(self.0.path())
            .schema("qa")
            .schema_file(self.0.path().join("schema.sql"))
            .build()
            .unwrap()
    }

    pub(crate) fn seed_inventory(&self, datasource: &str, content: &str) {
        std::fs::write(self.path(datasource), content).unwrap();
    }

    pub(crate) fn inventory_bytes(&self, datasource: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(datasource)).ok()
    }

    pub(crate) fn path(&self, datasource: &str) -> std::path::PathBuf {
        self.0.path().join(format!("sql-fun.{datasource}.qa.toml"))
    }
}

/// Preserve SQL-visible object kind without consulting definitions or SQL text.
pub(crate) fn object_response(kind: &str) -> AnalyzeQueryResponse {
    let relation = RelationName::new(None, Some("public"), "visible_object");
    let object = ::serde_json::from_value(::serde_json::json!(kind)).unwrap();
    let access = ColumnAccess::new(AccessKind::select_output(), object, &relation, Some("id"));
    AnalyzeQueryResponse::new(Collection::completed(vec![AnalyzeQueryItem::new(
        None,
        None,
        None,
        false,
        Collection::completed(Vec::new()),
        Collection::completed(Vec::new()),
        Collection::completed(Vec::new()),
        Collection::completed(vec![access]),
    )]))
}
