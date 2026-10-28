use super::{ConflictMasters, lookup_collation, lookup_operator_class};
use crate::context::BaseParseContext;
use crate::{
    context::ConflictInfoRead,
    sem::{CollationIdentity, FullName, ParseContext, SchemaFileContext, SchemaName},
};

#[rstest::rstest]
#[case::unqualified("C", None, Some("pg_catalog"))]
#[case::qualified("C", Some("pg_catalog"), Some("pg_catalog"))]
#[case::missing("absent", None, None)]
#[case::wrong_schema("C", Some("absent"), None)]
fn resolves_collation_names(
    #[case] name: &str,
    #[case] schema: Option<&str>,
    #[case] expected: Option<&str>,
) {
    let fixture = test_helpers::Masters::new();
    let context = fixture.context();
    let requested = if let Some(schema) = schema {
        FullName::with_schema(schema, name)
    } else {
        FullName::from_local_name(name)
    };
    let result = lookup_collation(&context, &requested);
    assert_eq!(result.requested(), &requested);
    assert_eq!(
        result.resolved_name(),
        expected
            .map(|schema| FullName::with_schema(schema, name))
            .as_ref()
    );
    assert_eq!(result.is_missing(), expected.is_none());
}

#[rstest::rstest]
#[case::specific(6, 6)]
#[case::independent(8, -1)]
fn preserves_encoding_identity_and_precedence(
    #[case] encoding: i32,
    #[case] selected_encoding: i32,
) {
    let mut fixture = test_helpers::Masters::new();
    fixture.set_encoding(encoding);
    fixture.add_collation("public", "variant", -1, "c");
    fixture.add_collation("public", "variant", 6, "c");
    let context = fixture.context();
    let name = FullName::with_schema("public", "variant");
    let definition = context.collation_definition(&name).unwrap();
    assert_eq!(definition.encoding(), selected_encoding);
    let identity = CollationIdentity::new(&name, definition.encoding()).unwrap();
    assert_eq!(identity.qualified_name(), definition.qualified_name());
    assert_eq!(identity.encoding(), definition.encoding());
    assert_eq!(context.database_collation().unwrap().encoding(), encoding);
}

#[rstest::rstest]
#[case::utf8(6, true)]
#[case::sql_ascii(0, false)]
#[case::euc_jis_2004(5, false)]
#[case::latin10(17, false)]
#[case::win874(21, false)]
fn filters_icu_encoding_eligibility(#[case] encoding: i32, #[case] found: bool) {
    let mut fixture = test_helpers::Masters::new();
    fixture.set_encoding(encoding);
    fixture.add_collation("public", "icu", -1, "i");
    assert_eq!(
        fixture
            .context()
            .collation_definition(&FullName::with_schema("public", "icu"))
            .is_some(),
        found
    );
}

#[test]
fn ignores_ineligible_schema_before_shadowing() {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("first", "choice", 8, "c");
    fixture.add_collation("second", "choice", -1, "c");
    let mut context = fixture.context();
    context.set_search_path(&[SchemaName::from("first"), SchemaName::from("second")]);
    let result = lookup_collation(&context, &FullName::from_local_name("choice"));
    assert_eq!(
        result.resolved_name(),
        Some(&FullName::with_schema("second", "choice"))
    );
}

#[rstest::rstest]
#[case::implicit_catalog(false, "pg_catalog")]
#[case::explicit_catalog(true, "public")]
fn honors_catalog_position(#[case] explicit_catalog: bool, #[case] expected: &str) {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("public", "C", -1, "c");
    let mut context = fixture.context();
    let mut path = vec![SchemaName::from("public")];
    if explicit_catalog {
        path.push(SchemaName::from("pg_catalog"));
    }
    context.set_search_path(&path);
    let result = lookup_collation(&context, &FullName::from_local_name("C"));
    assert_eq!(
        result.resolved_name(),
        Some(&FullName::with_schema(expected, "C"))
    );
}

#[test]
fn child_lookup_uses_current_path_and_freeze_preserves_it() {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("first", "Mixed.名", -1, "c");
    fixture.add_collation("second", "Mixed.名", -1, "c");
    let mut parent = fixture.context();
    parent.set_search_path(&[SchemaName::from("first")]);
    let args = parent.get_args().clone();
    let metadata = parent.metadata().clone();
    let frozen = parent.freeze();
    let mut child = SchemaFileContext::new(
        std::path::Path::new("selected.sql"),
        &frozen,
        &args,
        &metadata,
    );
    child.set_search_path(&[SchemaName::from("second")]);
    let requested = FullName::from_local_name("Mixed.名");
    assert_eq!(
        child.lookup_collation(&requested).resolved_name(),
        Some(&FullName::with_schema("second", "Mixed.名"))
    );
    let child = child.freeze();
    assert_eq!(
        child.lookup_collation(&requested).resolved_name(),
        Some(&FullName::with_schema("second", "Mixed.名"))
    );
    assert_eq!(
        lookup_collation(&frozen, &requested).resolved_name(),
        Some(&FullName::with_schema("first", "Mixed.名"))
    );
    assert_eq!(child.database_collation(), frozen.database_collation());
    let qualified = FullName::with_schema("second", "Mixed.名");
    assert_eq!(
        child.collation_definition(&qualified),
        frozen.collation_definition(&qualified)
    );
    assert!(child.collation_definition(&requested).is_none());
    assert!(
        child
            .operator_class_definition(&FullName::from_local_name("ops"), "btree")
            .is_none()
    );
    assert!(
        child
            .operator_class_definition(&FullName::with_schema("public", "ops"), "gin")
            .is_none()
    );
}

#[test]
fn operator_class_reads_retain_method_family_type_and_default() {
    let fixture = test_helpers::Masters::new();
    let context = fixture.context();
    let name = FullName::with_schema("public", "ops");
    for method in ["btree", "hash"] {
        let result = lookup_operator_class(&context, &FullName::from_local_name("ops"), method);
        assert_eq!(result.resolved_name(), Some(&name));
        let definition = context.operator_class_definition(&name, method).unwrap();
        assert_eq!(definition.family().access_method(), method);
        assert_eq!(
            definition.input_type().qualified_name(),
            FullName::with_schema("outside", "a_type")
        );
        assert!(definition.default());
        assert_eq!(definition.key_type(), &None);
    }
    assert!(lookup_operator_class(&context, &name, "gin").is_missing());
}

#[test]
fn frozen_custom_context_preserves_completed_ambiguity() {
    let fixture = test_helpers::Masters::new();
    let mut context = fixture.context();
    let requested = FullName::from_local_name("custom");
    let candidates = [
        FullName::with_schema("one", "custom"),
        FullName::with_schema("two", "custom"),
    ];
    let outcome = crate::sem::CatalogNameLookup::ambiguous(&requested, &candidates);
    context.set_collation_lookup(&requested, &outcome);
    let frozen = context.freeze();
    assert_eq!(frozen.lookup_collation(&requested), outcome);
    assert_eq!(
        frozen
            .clone()
            .lookup_collation(&requested)
            .ambiguous_candidates(),
        Some(candidates.as_slice())
    );
}

#[test]
fn unqualified_lookup_skips_temporary_namespaces() {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("pg_temp_42", "temp_only", -1, "c");
    let mut context = fixture.context();
    context.set_search_path(&[SchemaName::from("pg_temp_42")]);
    assert!(lookup_collation(&context, &FullName::from_local_name("temp_only")).is_missing());
    assert!(
        lookup_collation(&context, &FullName::with_schema("pg_temp_42", "temp_only"))
            .resolved_name()
            .is_some()
    );
}

#[rstest::rstest]
#[case::duplicate_collation("duplicate_collation")]
#[case::duplicate_class("duplicate_class")]
#[case::empty_classes("empty_classes")]
#[case::encoding("encoding")]
#[case::default_identity("default_identity")]
#[case::default_provider("default_provider")]
#[case::family_method("family_method")]
#[case::malformed("malformed")]
#[case::missing("missing")]
fn rejects_invalid_artifacts_with_paths(#[case] defect: &str) {
    let mut fixture = test_helpers::Masters::new();
    fixture.corrupt(defect);
    let error = fixture.load().unwrap_err().to_string();
    assert!(
        error.contains("collations.json") || error.contains("operator_classes.json"),
        "{error}"
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_namespace_is_explicit_case_insensitive_and_retained() -> testresult::TestResult {
    let mut masters = ConflictMasters::sqlite();
    masters.declare_sqlite_collation("Custom.名")?;
    masters.declare_sqlite_collation("CUSTOM.名")?;
    masters.declare_sqlite_collation("")?;
    let mut fixture = test_helpers::Masters::new()
        .context()
        .with_dialect(crate::ast::Dialect::sqlite());
    fixture.set_conflict_masters(&masters);
    let args = fixture.get_args().clone();
    let metadata = fixture.metadata().clone();
    let frozen = fixture.freeze();
    let child = SchemaFileContext::new(
        std::path::Path::new("sqlite.sql"),
        &frozen,
        &args,
        &metadata,
    )
    .freeze();
    for name in ["binary", "NoCase", "rtrim", "custom.名", ""] {
        assert!(
            child
                .lookup_collation(&FullName::from_local_name(name))
                .resolved_name()
                .is_some()
        );
    }
    assert!(
        child
            .lookup_collation(&FullName::from_local_name("unselected"))
            .is_missing()
    );
    assert!(
        child
            .lookup_collation(&FullName::with_schema("main", "BINARY"))
            .is_missing()
    );
    assert!(
        child
            .lookup_operator_class(&FullName::from_local_name("ops"), "btree")
            .is_missing()
    );
    assert!(masters.declare_sqlite_collation("bad\0name").is_err());
    let connection = ::rusqlite::Connection::open_in_memory()?;
    for name in ["binary", "NoCase", "rtrim"] {
        connection.prepare(&format!("SELECT 'x' COLLATE {name} = 'X'"))?;
    }
    assert!(
        connection
            .prepare("SELECT 'x' COLLATE unselected = 'X'")
            .is_err()
    );
    connection.create_collation("Custom.名", |left, right| left.cmp(right))?;
    connection.create_collation("", |left, right| left.cmp(right))?;
    connection.prepare(r#"SELECT 'x' COLLATE "" = 'X'"#)?;
    connection.prepare(r#"SELECT 'x' COLLATE "CUSTOM.名" = 'X'"#)?;
    eprintln!("bundled SQLite {}", ::rusqlite::version());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_base_needs_no_postgres_masters_and_exposes_registration() -> testresult::TestResult {
    let mut base = test_helpers::base_without_masters(crate::ast::Dialect::sqlite())?;
    base.declare_sqlite_collation("provided")?;
    let frozen = crate::sem::FrozenBaseContext::from(base);
    assert!(
        frozen
            .lookup_collation(&FullName::from_local_name("PROVIDED"))
            .resolved_name()
            .is_some()
    );
    assert!(frozen.database_collation().is_none());
    Ok(())
}

#[test]
fn postgres_base_requires_masters() {
    let error = test_helpers::base_without_masters(crate::ast::Dialect::postgres())
        .unwrap_err()
        .to_string();
    assert!(error.contains("collations.json"), "{error}");
}

#[cfg(feature = "sqlite")]
#[test]
#[should_panic(expected = "PostgreSQL master definitions must be supplied")]
fn unavailable_postgres_service_is_not_a_completed_missing_lookup() {
    let parent = test_helpers::base_without_masters(crate::ast::Dialect::sqlite()).unwrap();
    let args = parent.get_args().clone();
    let metadata = test_helpers::Masters::new().context().metadata().clone();
    let frozen = crate::sem::FrozenBaseContext::from(parent);
    let child = SchemaFileContext::new(
        std::path::Path::new("postgres.sql"),
        &frozen,
        &args,
        &metadata,
    );
    let _lookup = child.lookup_collation(&FullName::from_local_name("C"));
}

#[cfg(feature = "sqlite")]
#[test]
fn active_sqlite_dialect_does_not_use_frozen_postgres_namespace() {
    let fixture = test_helpers::Masters::new();
    let parent = fixture.context();
    let args = parent.get_args().clone();
    let sqlite = test_helpers::Masters::new()
        .context()
        .with_dialect(crate::ast::Dialect::sqlite());
    let frozen = parent.freeze();
    let child = SchemaFileContext::new(
        std::path::Path::new("sqlite.sql"),
        &frozen,
        &args,
        sqlite.metadata(),
    );
    assert!(
        child
            .lookup_collation(&FullName::from_local_name("binary"))
            .resolved_name()
            .is_some()
    );
    assert!(
        child
            .lookup_collation(&FullName::from_local_name("C"))
            .is_missing()
    );
    assert!(
        child
            .lookup_operator_class(&FullName::from_local_name("ops"), "btree")
            .is_missing()
    );
}

mod test_helpers {
    use super::ConflictMasters;
    use crate::test_helpers::TestParseContext;
    use ::serde_json::Value;

    pub(super) fn base_without_masters(
        dialect: crate::ast::Dialect,
    ) -> Result<crate::context::BaseContext, Box<dyn std::error::Error>> {
        let directory = ::tempfile::tempdir()?;
        for file in ["pg_type.json", "functions.json", "func_detail.json"] {
            std::fs::write(directory.path().join(file), "[]")?;
        }
        for file in [
            "binary_op.json",
            "left_unary_op.json",
            "right_unary_op.json",
            "casts.json",
        ] {
            std::fs::write(directory.path().join(file), "{}")?;
        }
        std::fs::write(
            directory.path().join("binary_op.json"),
            r#"{"+": [{"left":"int4", "right":"int4", "result":"int4", "strict":true}]}"#,
        )?;
        let args = ::sql_fun_core::SqlFunArgs::builder(&std::collections::HashMap::new())?
            .builtin_schema_dir(directory.path())
            .build()?;
        let metadata = TestParseContext::default().with_dialect(dialect);
        Ok(crate::context::BaseContext::new(
            &args,
            crate::context::BaseParseContext::metadata(&metadata),
        )?)
    }

    pub(super) struct Masters {
        directory: ::tempfile::TempDir,
        collations: Value,
        classes: Value,
        defect: Option<String>,
    }

    impl Masters {
        pub(super) fn new() -> Self {
            let collation = |name: &str, provider: &str| {
                ::serde_json::json!({"schema":"pg_catalog", "name": name, "encoding":-1,
                "provider":provider, "deterministic":true, "collate":null, "ctype":null, "locale":null, "icu_rules":null, "version":null})
            };
            let class = |method: &str| {
                ::serde_json::json!({"schema":"public", "name":"ops", "access_method":method,
                "family":{"schema":"outside", "name":"family", "access_method":method},
                "input_type":{"schema":"outside", "name":"a_type"}, "default":true, "key_type":null})
            };
            Self {
                directory: ::tempfile::tempdir().unwrap(),
                collations: ::serde_json::json!({"database":{"encoding":6,"encoding_name":"UTF8","provider":"c","collate":"C","ctype":"C", "locale":null,"icu_rules":null,"version":null,
                    "default_collation":{"schema":"pg_catalog","name":"default","encoding":-1}},
                    "collations":[collation("default", "d"), collation("C", "c")]}),
                classes: ::serde_json::json!([class("btree"), class("hash")]),
                defect: None,
            }
        }
        pub(super) fn set_encoding(&mut self, encoding: i32) {
            self.collations["database"]["encoding"] = encoding.into();
        }
        pub(super) fn add_collation(
            &mut self,
            schema: &str,
            name: &str,
            encoding: i32,
            provider: &str,
        ) {
            let mut value = self.collations["collations"][1].clone();
            value["schema"] = schema.into();
            value["name"] = name.into();
            value["encoding"] = encoding.into();
            value["provider"] = provider.into();
            self.collations["collations"]
                .as_array_mut()
                .unwrap()
                .push(value);
        }
        pub(super) fn corrupt(&mut self, defect: &str) {
            match defect {
                "duplicate_collation" => {
                    let value = self.collations["collations"][0].clone();
                    self.collations["collations"]
                        .as_array_mut()
                        .unwrap()
                        .push(value);
                }
                "duplicate_class" => {
                    let value = self.classes[0].clone();
                    self.classes.as_array_mut().unwrap().push(value);
                }
                "empty_classes" => self.classes = ::serde_json::json!([]),
                "encoding" => self.collations["database"]["encoding"] = 35.into(),
                "default_identity" => {
                    self.collations["database"]["default_collation"]["name"] = "absent".into()
                }
                "default_provider" => self.collations["collations"][0]["provider"] = "c".into(),
                "family_method" => self.classes[0]["family"]["access_method"] = "gin".into(),
                "malformed" | "missing" => self.defect = Some(defect.to_owned()),
                _ => panic!("unknown fixture defect"),
            }
        }
        pub(super) fn load(&self) -> Result<ConflictMasters, crate::sem::AnalysisError> {
            let collations = self.directory.path().join("collations.json");
            let classes = self.directory.path().join("operator_classes.json");
            std::fs::write(&collations, self.collations.to_string()).unwrap();
            std::fs::write(&classes, self.classes.to_string()).unwrap();
            if self.defect.as_deref() == Some("malformed") {
                std::fs::write(&classes, "[").unwrap();
            }
            if self.defect.as_deref() == Some("missing") {
                std::fs::remove_file(&collations).unwrap();
            }
            ConflictMasters::load(&collations, &classes)
        }
        pub(super) fn context(&self) -> TestParseContext {
            let mut context = TestParseContext::default();
            context.set_conflict_masters(&self.load().unwrap());
            crate::sem::ParseContext::set_search_path(
                &mut context,
                &[crate::sem::SchemaName::from("public")],
            );
            context
        }
    }
}
