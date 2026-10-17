use std::path::Path;

pub(super) struct CatalogProject {
    directory: tempfile::TempDir,
}

impl CatalogProject {
    pub(super) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("Cargo.toml"), "[package.metadata.database.default]\nengine = \"PostgreSQL\"\ndialect = \"postgres\"\n").unwrap();
        Self { directory }
    }

    pub(super) fn arguments(&self, catalog: &Path) -> sql_fun_core::SqlFunArgs {
        sql_fun_core::SqlFunArgsBuilder::default()
            .metadata_file(self.directory.path().join("Cargo.toml").to_str().unwrap())
            .builtin_schema_dir(catalog.to_owned())
            .build()
            .unwrap()
    }
}

/// Lightweight JSON-only inspection of committed producer output; no semantic bootstrap.
pub(super) struct MasterDefinitions {
    collations: serde_json::Value,
    operator_classes: serde_json::Value,
}

impl MasterDefinitions {
    pub(super) fn builtin(version: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("sql-fun-home/postgres")
            .join(version);
        let read = |name| serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
        Self {
            collations: read("collations.json"),
            operator_classes: read("operator_classes.json"),
        }
    }

    pub(super) fn assert_contract(&self) {
        let database = &self.collations["database"];
        assert!(database["encoding"].is_u64());
        assert!(database["encoding_name"].is_string());
        assert!(database["provider"].is_string());
        let mut identities = std::collections::BTreeSet::new();
        let mut previous = None;
        let collations = self.collations["collations"].as_array().unwrap();
        for collation in collations {
            let identity = (
                collation["schema"].as_str().unwrap(),
                collation["name"].as_str().unwrap(),
                collation["encoding"].as_i64().unwrap(),
            );
            assert!(identities.insert(identity), "duplicate collation identity");
            if let Some(previous) = previous {
                assert!(previous < identity);
            }
            previous = Some(identity);
            assert_eq!(identity.0, "pg_catalog");
            assert!(collation["deterministic"].is_boolean());
            assert!(collation["provider"].is_string());
            for property in ["collate", "ctype", "locale", "icu_rules", "version"] {
                let value = collation.get(property).expect("required nullable property");
                assert!(value.is_null() || value.is_string());
            }
        }
        let default = &database["default_collation"];
        assert!(collations.iter().any(|c| c["schema"] == default["schema"]
            && c["name"] == default["name"]
            && c["encoding"] == default["encoding"]
            && c["provider"] == "d"));
        assert!(
            collations
                .iter()
                .any(|c| c["name"] == "C" && c["encoding"] == -1)
        );
        let mut identities = std::collections::BTreeSet::new();
        let mut previous = None;
        let classes = self.operator_classes.as_array().unwrap();
        for class in classes {
            let identity = (
                class["schema"].as_str().unwrap(),
                class["name"].as_str().unwrap(),
                class["access_method"].as_str().unwrap(),
            );
            assert!(
                identities.insert(identity),
                "duplicate operator-class identity"
            );
            if let Some(previous) = previous {
                assert!(previous < identity);
            }
            previous = Some(identity);
            assert_eq!(identity.0, "pg_catalog");
            assert!(class["default"].is_boolean());
            assert_eq!(class["family"]["access_method"], class["access_method"]);
            for field in ["family", "input_type", "key_type"] {
                let value = class.get(field).expect("required relationship");
                if field == "key_type" && value.is_null() {
                    continue;
                }
                assert!(value["schema"].is_string());
                assert!(value["name"].is_string());
            }
        }
        assert!(classes.iter().any(|c| c["name"] == "int4_ops"
            && c["access_method"] == "btree"
            && c["default"] == true
            && c["family"]["name"] == "integer_ops"
            && c["input_type"]["name"] == "int4"
            && c["key_type"].is_null()));
        assert!(classes.iter().any(|c| c["default"] == false));
        assert!(classes.iter().any(|c| c["key_type"].is_object()));
    }
}
