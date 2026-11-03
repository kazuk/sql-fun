#[cfg(feature = "sqlite")]
use crate::context::ConflictMasters;
#[cfg(feature = "sqlite")]
use crate::context::FrozenBaseContext;
#[cfg(feature = "sqlite")]
use crate::context::{BaseParseContext, ParseContext, SchemaFileContext};
use crate::{
    ast::Dialect, context::ConflictInfoRead, names::FullName, sem::CollationIdentity,
    test_helpers::conflict_masters as test_helpers,
};

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
        .with_dialect(Dialect::sqlite());
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
    let mut base = test_helpers::base_without_masters(Dialect::sqlite())?;
    base.declare_sqlite_collation("provided")?;
    let frozen = FrozenBaseContext::from(base);
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
    let error = test_helpers::base_without_masters(Dialect::postgres())
        .unwrap_err()
        .to_string();
    assert!(error.contains("collations.json"), "{error}");
}
