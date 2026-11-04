#[cfg(feature = "sqlite")]
use crate::context::ConflictMasters;
#[cfg(feature = "sqlite")]
use crate::context::FrozenBaseContext;
#[cfg(feature = "sqlite")]
use crate::context::{BaseParseContext, ParseContext, SchemaFileContext};
use crate::{
    ast::Dialect, context::ConflictInfoRead, names::FullName,
    test_helpers::conflict_masters as test_helpers,
};

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
