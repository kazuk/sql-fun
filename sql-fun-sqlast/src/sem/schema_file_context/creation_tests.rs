//! Parser-to-catalog regression tests using small, explicitly seeded catalogs.
use crate::sem::{
    ColumnName, FullName, Kind, ObjectInfoRead, OwnershipInfoRead, ParseContext, TableInfoRead,
    TypeInfoRead, ViewInfoRead,
};

#[rstest::rstest]
#[case::table_table(
    "CREATE TABLE public.t(id integer)",
    "CREATE TABLE public.t(other integer)",
    Kind::table(),
    Kind::table()
)]
#[case::table_view(
    "CREATE TABLE public.t(id integer)",
    "CREATE VIEW public.t AS SELECT 1 AS id",
    Kind::table(),
    Kind::view()
)]
#[case::view_table(
    "CREATE VIEW public.t AS SELECT 1 AS id",
    "CREATE TABLE public.t(other integer)",
    Kind::view(),
    Kind::table()
)]
#[case::sequence_table(
    "CREATE SEQUENCE public.t",
    "CREATE TABLE public.t(id integer)",
    Kind::sequence(),
    Kind::table()
)]
#[case::table_sequence(
    "CREATE TABLE public.t(id integer)",
    "CREATE SEQUENCE public.t",
    Kind::table(),
    Kind::sequence()
)]
#[case::index_table(
    "CREATE TABLE public.items(id integer); CREATE INDEX t ON public.items(id)",
    "CREATE TABLE public.t(id integer)",
    Kind::index(),
    Kind::table()
)]
#[case::table_index(
    "CREATE TABLE public.t(id integer)",
    "CREATE INDEX t ON public.t(id)",
    Kind::table(),
    Kind::index()
)]
#[case::matview_table(
    "CREATE MATERIALIZED VIEW public.t AS SELECT 1 AS id",
    "CREATE TABLE public.t(id integer)",
    Kind::materialized_view(),
    Kind::table()
)]
#[case::table_matview(
    "CREATE TABLE public.t(id integer)",
    "CREATE MATERIALIZED VIEW public.t AS SELECT 1 AS id",
    Kind::table(),
    Kind::materialized_view()
)]
fn rejects_shared_relation_names(
    #[case] first: &str,
    #[case] second: &str,
    #[case] existing: Kind,
    #[case] requested: Kind,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let mut context = test_helpers::postgres(first)?;
    if inherited {
        context = test_helpers::child(context.freeze());
    }
    context = test_helpers::apply(context, second)?;
    assert_eq!(context.problems().len(), 1, "{:?}", context.problems());
    let problem = context.problems()[0]
        .creation_problem()
        .expect("creation conflict");
    assert_eq!(
        problem
            .existing_kind()
            .map(::sql_fun_core::analysis_problem::Kind::as_str),
        Some(existing.to_string().as_str())
    );
    assert_eq!(
        problem
            .requested_kind()
            .map(::sql_fun_core::analysis_problem::Kind::as_str),
        Some(requested.to_string().as_str())
    );
    assert_eq!(problem.name().as_str(), "public.t");
    assert_eq!(
        context.object_kind(&FullName::with_schema("public", "t")),
        Some(existing)
    );
    context = test_helpers::apply(context, "CREATE TABLE public.later(id integer)")?;
    assert_eq!(
        context.object_kind(&FullName::with_schema("public", "later")),
        Some(Kind::table())
    );
    assert_eq!(context.problems().len(), 1);
    Ok(())
}

#[rstest::rstest]
#[case::table(
    "CREATE TABLE public.t(id integer)",
    "CREATE TABLE IF NOT EXISTS public.t(other integer)"
)]
#[case::sequence(
    "CREATE TABLE public.t(id integer)",
    "CREATE SEQUENCE IF NOT EXISTS public.t"
)]
#[case::index(
    "CREATE TABLE public.t(id integer)",
    "CREATE INDEX IF NOT EXISTS t ON public.t(id)"
)]
#[case::matview(
    "CREATE TABLE public.t(id integer)",
    "CREATE MATERIALIZED VIEW IF NOT EXISTS public.t AS SELECT 1 AS other"
)]
fn idempotent_creation_keeps_the_original(
    #[case] first: &str,
    #[case] second: &str,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let mut context = test_helpers::postgres(first)?;
    if inherited {
        context = test_helpers::child(context.freeze());
    }
    let context = test_helpers::apply(context, second)?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let name = FullName::with_schema("public", "t");
    assert_eq!(context.object_kind(&name), Some(Kind::table()));
    assert!(
        context
            .get_table_impl(&name)
            .expect("original table")
            .has_column(&ColumnName::from("id"))
    );
    Ok(())
}

#[rstest::rstest]
#[case::enum_type(
    "CREATE TYPE public.t AS ENUM ('first')",
    "CREATE TYPE public.t AS ENUM ('second')"
)]
#[case::domain(
    "CREATE DOMAIN public.t AS integer",
    "CREATE DOMAIN public.t AS integer"
)]
#[case::composite(
    "CREATE TYPE public.t AS (id integer)",
    "CREATE TYPE public.t AS (other integer)"
)]
#[case::row_type(
    "CREATE TABLE public.t(id integer)",
    "CREATE TYPE public.t AS ENUM ('second')"
)]
#[case::type_blocks_table(
    "CREATE TYPE public.t AS ENUM ('first')",
    "CREATE TABLE public.t(id integer)"
)]
#[case::view_row_type(
    "CREATE VIEW public.t AS SELECT 1 AS id",
    "CREATE TYPE public.t AS ENUM ('second')"
)]
fn rejects_type_namespace_conflicts(
    #[case] first: &str,
    #[case] second: &str,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let mut context = test_helpers::postgres(first)?;
    if inherited {
        context = test_helpers::child(context.freeze());
    }
    let context = test_helpers::apply(context, second)?;
    assert_eq!(context.problems().len(), 1, "{:?}", context.problems());
    assert_eq!(
        context.problems()[0]
            .creation_problem()
            .expect("type conflict")
            .namespace(),
        "type"
    );
    assert!(
        context
            .get_type_impl(&FullName::with_schema("public", "t"))
            .is_some()
    );
    Ok(())
}

#[rstest::rstest]
#[case::schema("CREATE SCHEMA s", "CREATE SCHEMA s", false)]
#[case::schema_skip("CREATE SCHEMA s", "CREATE SCHEMA IF NOT EXISTS s", true)]
#[case::extension("CREATE EXTENSION uninstalled", "CREATE EXTENSION uninstalled", false)]
#[case::extension_skip(
    "CREATE EXTENSION uninstalled",
    "CREATE EXTENSION IF NOT EXISTS uninstalled",
    true
)]
fn separate_database_namespaces(
    #[case] first: &str,
    #[case] second: &str,
    #[case] skip: bool,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let mut context = test_helpers::postgres(first)?;
    if inherited {
        context = test_helpers::child(context.freeze());
    }
    let context = test_helpers::apply(context, second)?;
    assert_eq!(
        context.problems().len(),
        usize::from(!skip),
        "{:?}",
        context.problems()
    );
    Ok(())
}

#[test]
fn rejected_alter_preserves_table_indexes_and_parent() -> testresult::TestResult {
    let parent = test_helpers::postgres(
        "CREATE TABLE public.items(id integer); CREATE TABLE public.occupied(id integer)",
    )?
    .freeze();
    let child = test_helpers::child(parent.clone());
    let child = test_helpers::apply(
        child,
        "ALTER TABLE public.items ADD CONSTRAINT occupied UNIQUE(id)",
    )?;
    assert_eq!(child.problems().len(), 1);
    let items = FullName::with_schema("public", "items");
    assert!(
        child
            .get_table_impl(&items)
            .expect("table")
            .constraint("occupied")
            .is_none()
    );
    assert!(
        parent
            .get_table_impl(&items)
            .expect("parent")
            .constraint("occupied")
            .is_none()
    );
    assert_eq!(
        child.object_kind(&FullName::with_schema("public", "occupied")),
        Some(Kind::table())
    );
    let child = test_helpers::apply(
        child,
        "ALTER TABLE public.items ADD CONSTRAINT valid UNIQUE(id)",
    )?;
    assert!(
        child
            .get_table_impl(&items)
            .expect("table")
            .constraint("valid")
            .is_some()
    );
    assert_eq!(
        child.object_kind(&FullName::with_schema("public", "valid")),
        Some(Kind::index())
    );
    assert_eq!(child.problems().len(), 1);
    Ok(())
}

#[test]
fn rejected_create_does_not_publish_row_type_or_implicit_index() -> testresult::TestResult {
    let context = test_helpers::postgres("CREATE TABLE public.occupied(id integer)")?;
    let context = test_helpers::apply(
        context,
        "CREATE TABLE public.items(id integer, CONSTRAINT occupied UNIQUE(id))",
    )?;
    let name = FullName::with_schema("public", "items");
    assert_eq!(context.problems().len(), 1);
    assert!(context.object_kind(&name).is_none());
    assert!(context.get_table_impl(&name).is_none());
    assert!(context.get_type_impl(&name).is_none());
    Ok(())
}

#[test]
fn legal_namespaces_and_qualified_identities_remain_distinct() -> testresult::TestResult {
    let context = test_helpers::postgres(
        r#"
        CREATE TABLE a.t(id integer);
        CREATE TABLE b.t(id integer);
        CREATE TABLE "a.b".t(id integer);
        CREATE TABLE a."b.t"(id integer);
        CREATE FUNCTION a.t(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1';
        CREATE FUNCTION a.t(text) RETURNS text LANGUAGE sql AS 'SELECT $1';
        CREATE TYPE a.idx AS ENUM ('x');
        CREATE INDEX idx ON a.t(id);
    "#,
    )?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(
        context
            .routine_definitions(&FullName::with_schema("a", "t"))
            .len(),
        2
    );
    assert_eq!(
        context.object_kind(&FullName::with_schema("a", "idx")),
        Some(Kind::index())
    );
    Ok(())
}

#[rstest::rstest]
#[case::duplicate(
    "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
    false
)]
#[case::replace(
    "CREATE OR REPLACE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1 + 1'",
    true
)]
#[case::different_result(
    "CREATE OR REPLACE FUNCTION public.f(integer) RETURNS text LANGUAGE sql AS 'SELECT $1::text'",
    false
)]
#[case::procedure("CREATE PROCEDURE public.f(integer) LANGUAGE sql AS 'SELECT 1'", false)]
#[case::overload(
    "CREATE FUNCTION public.f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
    true
)]
fn routine_signature_policy(
    #[case] sql: &str,
    #[case] accepted: bool,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let mut context = test_helpers::postgres(
        "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
    )?;
    if inherited {
        context = test_helpers::child(context.freeze());
    }
    let context = test_helpers::apply(context, sql)?;
    assert_eq!(
        context.problems().len(),
        usize::from(!accepted),
        "{:?}",
        context.problems()
    );
    assert!(
        context
            .object_kind(&FullName::with_schema("public", "f"))
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
#[case::compatible("CREATE OR REPLACE VIEW public.v AS SELECT 2 AS id, 3 AS added", true)]
#[case::rename("CREATE OR REPLACE VIEW public.v AS SELECT 2 AS other", false)]
#[case::kind("CREATE TABLE public.v(id integer)", false)]
fn view_replacement_policy(#[case] sql: &str, #[case] accepted: bool) -> testresult::TestResult {
    let context = test_helpers::postgres("CREATE VIEW public.v AS SELECT 1 AS id")?;
    let context = test_helpers::child(context.freeze());
    let context = test_helpers::apply(context, sql)?;
    assert_eq!(context.problems().len(), usize::from(!accepted));
    assert!(
        context
            .get_view_impl(&FullName::with_schema("public", "v"))
            .expect("original view")
            .has_column(&ColumnName::from("id"))
    );
    Ok(())
}

#[test]
fn record_only_drop_retains_the_name() -> testresult::TestResult {
    let context = test_helpers::postgres(
        "CREATE TABLE public.t(id integer); DROP TABLE public.t; CREATE TABLE public.t(other integer)",
    )?;
    assert_eq!(context.problems().len(), 1);
    assert_eq!(
        context.object_kind(&FullName::with_schema("public", "t")),
        Some(Kind::table())
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use super::test_helpers;
    use crate::sem::{FullName, Kind, ObjectInfoRead, ParseContext};

    #[rstest::rstest]
    #[case::duplicate("CREATE TABLE t(id)", "CREATE TABLE T(other)", false)]
    #[case::quoted_case(r#"CREATE TABLE "T"(id)"#, "CREATE TABLE t(other)", false)]
    #[case::cross_kind("CREATE VIEW t AS SELECT 1 AS id", "CREATE TABLE t(id)", false)]
    #[case::table_skip("CREATE TABLE t(id)", "CREATE TABLE IF NOT EXISTS t(other)", true)]
    #[case::view_skip(
        "CREATE VIEW t AS SELECT 1 AS id",
        "CREATE TABLE IF NOT EXISTS t(other)",
        true
    )]
    #[case::view_idempotent(
        "CREATE TABLE t(id)",
        "CREATE VIEW IF NOT EXISTS t AS SELECT 1 AS other",
        true
    )]
    fn collision_policy(
        #[case] first: &str,
        #[case] second: &str,
        #[case] accepted: bool,
        #[values(false, true)] inherited: bool,
    ) -> testresult::TestResult {
        assert_eq!(test_helpers::sqlite_accepts(first, second)?, accepted);
        let mut context = test_helpers::sqlite_context();
        context = test_helpers::apply_sqlite(context, first)?;
        if inherited {
            context = test_helpers::child(context.freeze());
        }
        let original = context.object_kind(&FullName::with_schema("main", "t"));
        let context = test_helpers::apply_sqlite(context, second)?;
        assert_eq!(
            context.problems().len(),
            usize::from(!accepted),
            "{:?}",
            context.problems()
        );
        assert!(original == Some(Kind::table()) || original == Some(Kind::view()));
        assert_eq!(
            context.object_kind(&FullName::with_schema("main", "t")),
            original
        );
        Ok(())
    }
}

mod test_helpers {
    #[cfg(feature = "sqlite")]
    use crate::sem::ParseContext;
    use crate::sem::{AnalysisError, FrozenBaseContext, MutSourcePathWithSpan, SchemaFileContext};
    pub(super) fn child(parent: FrozenBaseContext) -> SchemaFileContext {
        use crate::context::BaseParseContext;
        let mut context = SchemaFileContext::new(
            std::path::Path::new("creation.sql"),
            &parent,
            parent.get_args(),
            parent.metadata(),
        );
        context.set_source_path_with_span(&crate::test_helpers::source_span());
        context
    }
    pub(super) fn postgres(sql: &str) -> Result<SchemaFileContext, AnalysisError> {
        apply(crate::test_helpers::ownership::ownership_context(), sql)
    }
    pub(super) fn apply(
        mut context: SchemaFileContext,
        sql: &str,
    ) -> Result<SchemaFileContext, AnalysisError> {
        use crate::ast::TryFromWithTokens;
        let parsed = ::pg_query::parse(sql).expect("valid PostgreSQL fixture");
        for raw in parsed.protobuf.stmts {
            let node = crate::syn::Node::from(raw.stmt.expect("statement").as_ref().clone());
            let syntax = crate::ast::StatementNode::try_from_with_tokens(
                &node,
                &::sql_fun_core::IVec::default(),
            )?;
            context = syntax.resolve_and_apply(context)?.into_context();
        }
        Ok(context)
    }
    #[cfg(feature = "sqlite")]
    pub(super) fn sqlite_accepts(first: &str, second: &str) -> Result<bool, ::rusqlite::Error> {
        let connection = ::rusqlite::Connection::open_in_memory()?;
        connection.execute_batch(first)?;
        Ok(connection.execute_batch(second).is_ok())
    }
    #[cfg(feature = "sqlite")]
    pub(super) fn sqlite_context() -> SchemaFileContext {
        let mut root = crate::test_helpers::TestParseContext::default()
            .with_dialect(crate::ast::Dialect::sqlite());
        root.set_search_path(&[crate::sem::SchemaName::from("main")]);
        child(root.freeze())
    }
    #[cfg(feature = "sqlite")]
    pub(super) fn apply_sqlite(
        context: SchemaFileContext,
        sql: &str,
    ) -> Result<SchemaFileContext, AnalysisError> {
        let statement = crate::test_helpers::SqliteSynBuilder.parse_as_stmt(sql);
        Ok(crate::ast::StatementNode::try_from(&statement)?
            .resolve_and_apply(context)?
            .into_context())
    }
}

#[rstest::rstest]
#[case::composite_blocks_index(
    "CREATE TYPE public.t AS (id integer); CREATE TABLE public.items(id integer)",
    "CREATE INDEX t ON public.items(id)"
)]
#[case::index_blocks_composite(
    "CREATE TABLE public.items(id integer); CREATE INDEX t ON public.items(id)",
    "CREATE TYPE public.t AS (id integer)"
)]
fn composite_backing_relation_shares_index_names(
    #[case] first: &str,
    #[case] second: &str,
) -> testresult::TestResult {
    let context = test_helpers::postgres(first)?;
    let context = test_helpers::apply(test_helpers::child(context.freeze()), second)?;
    assert_eq!(context.problems().len(), 1);
    assert_eq!(
        context.problems()[0]
            .creation_problem()
            .expect("relation collision")
            .namespace(),
        "relation"
    );
    Ok(())
}

#[test]
fn later_implicit_collision_rejects_the_entire_alter() -> testresult::TestResult {
    let context = test_helpers::postgres(
        "CREATE TABLE public.items(id integer); CREATE TABLE public.occupied(id integer)",
    )?;
    let context = test_helpers::apply(
        context,
        "ALTER TABLE public.items ADD CONSTRAINT fresh UNIQUE(id), ADD CONSTRAINT occupied UNIQUE(id)",
    )?;
    assert_eq!(context.problems().len(), 1);
    assert!(
        context
            .object_kind(&FullName::with_schema("public", "fresh"))
            .is_none()
    );
    assert!(
        context
            .get_table_impl(&FullName::with_schema("public", "items"))
            .expect("table")
            .constraint("fresh")
            .is_none()
    );
    Ok(())
}

#[test]
fn explicit_temporary_table_uses_a_separate_creation_namespace() -> testresult::TestResult {
    let context =
        test_helpers::postgres("CREATE TABLE t(id integer); CREATE TEMP TABLE t(other integer)")?;
    assert!(context.problems().is_empty());
    assert_eq!(
        context.object_kind(&FullName::with_schema("public", "t")),
        Some(Kind::table())
    );
    assert_eq!(
        context.object_kind(&FullName::with_schema("pg_temp", "t")),
        Some(Kind::table())
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_separates_database_names_and_only_folds_ascii() -> testresult::TestResult {
    let mut context = test_helpers::sqlite_context();
    for sql in [
        r#"CREATE TABLE main.t(id)"#,
        r#"CREATE TEMP TABLE t(other)"#,
        r#"CREATE TABLE "Ä"(id)"#,
        r#"CREATE TABLE "ä"(id)"#,
    ] {
        context = test_helpers::apply_sqlite(context, sql)?;
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    for name in [
        FullName::with_schema("main", "t"),
        FullName::with_schema("temp", "t"),
        FullName::with_schema("main", "Ä"),
        FullName::with_schema("main", "ä"),
    ] {
        assert_eq!(context.object_kind(&name), Some(Kind::table()));
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_reserved_names_are_rejected_without_publication() -> testresult::TestResult {
    let context = test_helpers::apply_sqlite(
        test_helpers::sqlite_context(),
        "CREATE TABLE SQLite_reserved(id)",
    )?;
    assert_eq!(context.problems().len(), 1);
    assert!(
        context
            .object_kind(&FullName::with_schema("main", "sqlite_reserved"))
            .is_none()
    );
    Ok(())
}

#[test]
fn rejected_nested_schema_rolls_back_all_definitions() -> testresult::TestResult {
    let context = test_helpers::postgres(
        "CREATE SCHEMA bundle CREATE TABLE nested(id integer) CREATE TABLE nested(other integer)",
    )?;
    assert_eq!(context.problems().len(), 1);
    assert!(context.schema_definition("bundle").is_none());
    let name = FullName::with_schema("bundle", "nested");
    assert!(context.object_kind(&name).is_none());
    assert!(context.get_table_impl(&name).is_none());
    assert!(context.get_type_impl(&name).is_none());
    let context = test_helpers::apply(context, "CREATE TABLE bundle.nested(id integer)")?;
    assert_eq!(context.object_kind(&name), Some(Kind::table()));
    assert_eq!(context.problems().len(), 1);
    Ok(())
}

#[rstest::rstest]
#[case::plain("duplicate")]
#[case::spaced(r#""has space""#)]
#[case::dotted(r#""a.b""#)]
#[case::quoted(r#""a""b""#)]
fn duplicate_declared_constraint_is_recoverable(#[case] name: &str) -> testresult::TestResult {
    let context = test_helpers::postgres(&format!(
        "CREATE TABLE public.items(id integer, CONSTRAINT {name} UNIQUE(id), CONSTRAINT {name} UNIQUE(id)); CREATE TABLE public.later(id integer)",
    ))?;
    assert_eq!(context.problems().len(), 1);
    let problem = context.problems()[0]
        .creation_problem()
        .expect("constraint conflict");
    assert_eq!(problem.namespace(), "constraint");
    assert_eq!(problem.name().as_str(), name);
    assert_eq!(
        context.problems()[0]
            .render_metadata()
            .expect("constraint diagnostic")
            .message(),
        format!("conflicting constraint definition: {name}"),
    );
    assert_eq!(
        problem
            .container()
            .map(::sql_fun_core::analysis_problem::FullName::as_str),
        Some("public.items")
    );
    assert!(
        context
            .object_kind(&FullName::with_schema("public", "items"))
            .is_none()
    );
    assert_eq!(
        context.object_kind(&FullName::with_schema("public", "later")),
        Some(Kind::table())
    );
    Ok(())
}

#[test]
fn duplicate_altered_constraint_retains_canonical_name() -> testresult::TestResult {
    let context = test_helpers::postgres(
        r#"CREATE TABLE public.items(id integer, CONSTRAINT "a.b""c" UNIQUE(id)); ALTER TABLE public.items ADD CONSTRAINT "a.b""c" UNIQUE(id)"#,
    )?;
    assert_eq!(context.problems().len(), 1);
    let diagnostic = &context.problems()[0];
    assert_eq!(diagnostic.code(), "conflicting_creation");
    assert_eq!(
        diagnostic
            .creation_problem()
            .expect("constraint conflict")
            .name()
            .as_str(),
        r#""a.b""c""#,
    );
    assert_eq!(
        diagnostic
            .render_metadata()
            .expect("constraint diagnostic")
            .message(),
        r#"conflicting constraint definition: "a.b""c""#,
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_skipped_view_does_not_resolve_the_query() -> testresult::TestResult {
    let context = test_helpers::apply_sqlite(test_helpers::sqlite_context(), "CREATE TABLE t(id)")?;
    let context = test_helpers::apply_sqlite(
        context,
        "CREATE VIEW IF NOT EXISTS t AS SELECT missing FROM absent",
    )?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(
        context.object_kind(&FullName::with_schema("main", "t")),
        Some(Kind::table())
    );
    Ok(())
}

#[rstest::rstest]
#[case::implicit(&["public"], "pg_temp")]
#[case::explicit_late(&["public", "pg_temp"], "public")]
#[case::explicit_first(&["pg_temp", "public"], "pg_temp")]
fn temporary_relations_and_row_types_follow_search_path(
    #[case] path: &[&str],
    #[case] selected: &str,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    use crate::context::BaseParseContext;
    use crate::sem::{SchemaName, TableName};
    let mut context = test_helpers::postgres(
        "CREATE TABLE public.t(permanent integer); CREATE TEMP TABLE t(temporary integer)",
    )?;
    if inherited {
        context = test_helpers::child(context.freeze());
    }
    let mut schemas = Vec::new();
    for schema in path {
        schemas.push(SchemaName::from(schema));
    }
    context.set_search_path(&schemas);
    let name = FullName::from_local_name("t");
    assert_eq!(
        context
            .resolve_table_name(&TableName::from(name.clone()))
            .expect("table")
            .full_name(),
        &FullName::with_schema(selected, "t")
    );
    assert_eq!(
        context.resolve_type_name(&name),
        Some(FullName::with_schema(selected, "t"))
    );
    let column = if selected == "pg_temp" {
        "temporary"
    } else {
        "permanent"
    };
    let context = test_helpers::apply(context, &format!("SELECT {column} FROM t"))?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(())
}

#[test]
fn temporary_view_shadows_permanent_table_without_affecting_routines() -> testresult::TestResult {
    use crate::context::BaseParseContext;
    use crate::sem::{SchemaName, TableName};
    let context = test_helpers::postgres(
        "CREATE TABLE public.t(permanent integer); CREATE TEMP VIEW t AS SELECT 1 AS temporary; CREATE FUNCTION public.f() RETURNS integer LANGUAGE SQL AS 'SELECT 1'; CREATE FUNCTION pg_temp.f() RETURNS bigint LANGUAGE SQL AS 'SELECT 1'",
    )?;
    assert!(
        context
            .get_table(&TableName::from(FullName::from_local_name("t")))
            .is_none()
    );
    let mut context = test_helpers::apply(context, "SELECT temporary FROM t")?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    context = test_helpers::child(context.freeze());
    context.set_search_path(&[SchemaName::from("pg_temp"), SchemaName::from("public")]);
    assert_eq!(
        context.resolve_function_name_with_search_path(&FullName::from_local_name("f")),
        FullName::with_schema("public", "f")
    );
    Ok(())
}

#[rstest::rstest]
#[case::temporary("CREATE TEMP TABLE t AS SELECT 1 AS temporary", "pg_temp", "temporary")]
#[case::aliases(
    "CREATE TABLE t(renamed) AS SELECT 1 AS original, 2 AS second",
    "public",
    "renamed"
)]
fn table_as_retains_creation_identity_and_column_names(
    #[case] sql: &str,
    #[case] schema: &str,
    #[case] column: &str,
) -> testresult::TestResult {
    let context = if schema == "pg_temp" {
        let context = test_helpers::postgres("CREATE TABLE public.t(permanent integer)")?;
        test_helpers::apply(context, sql)?
    } else {
        test_helpers::postgres(sql)?
    };
    let table = context
        .get_table_impl(&FullName::with_schema(schema, "t"))
        .expect("created table");
    assert!(table.has_column(&ColumnName::from(column)));
    if column == "renamed" {
        assert!(!table.has_column(&ColumnName::from("original")));
        assert!(table.has_column(&ColumnName::from("second")));
    }
    let context = test_helpers::apply(context, &format!("SELECT {column} FROM t"))?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(())
}

#[rstest::rstest]
#[case::too_many("CREATE TABLE t(a,b) AS SELECT 1 AS original")]
#[case::duplicate("CREATE TABLE t(a,a) AS SELECT 1,2")]
#[case::duplicate_with_unrenamed("CREATE TABLE t(second) AS SELECT 1 AS first,2 AS second")]
fn invalid_table_as_column_names_do_not_publish_definitions(
    #[case] sql: &str,
) -> testresult::TestResult {
    let context = test_helpers::postgres(sql)?;
    assert_eq!(context.problems().len(), 1);
    let name = FullName::with_schema("public", "t");
    assert!(context.object_kind(&name).is_none());
    assert!(context.get_table_impl(&name).is_none());
    assert!(context.get_type_impl(&name).is_none());
    let context = test_helpers::apply(context, "CREATE TABLE t(valid integer)")?;
    assert_eq!(context.object_kind(&name), Some(Kind::table()));
    Ok(())
}

#[rstest::rstest]
#[case::check(
    "CREATE TABLE t(a integer CONSTRAINT same CHECK (a>0), b integer CONSTRAINT same CHECK (b>0))"
)]
#[case::foreign_key(
    "CREATE TABLE t(a integer CONSTRAINT same REFERENCES parent(id), b integer CONSTRAINT same REFERENCES parent(id))"
)]
#[case::mixed("CREATE TABLE t(a integer CONSTRAINT same CHECK (a>0), CONSTRAINT same UNIQUE(a))")]
fn all_named_column_constraints_participate_in_collision_checks(
    #[case] sql: &str,
) -> testresult::TestResult {
    let context = test_helpers::postgres(sql)?;
    assert_eq!(context.problems().len(), 1);
    let problem = context.problems()[0]
        .creation_problem()
        .expect("constraint conflict");
    assert_eq!(problem.namespace(), "constraint");
    assert_eq!(problem.name().as_str(), "same");
    assert!(
        context
            .object_kind(&FullName::with_schema("public", "t"))
            .is_none()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_unqualified_lookup_prefers_explicitly_created_temp_table() -> testresult::TestResult {
    let context =
        test_helpers::apply_sqlite(test_helpers::sqlite_context(), "CREATE TABLE t(permanent)")?;
    let context = test_helpers::apply_sqlite(context, "CREATE TEMP TABLE t(temporary)")?;
    let context = test_helpers::apply_sqlite(context, "SELECT temporary FROM t")?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(())
}

#[test]
fn named_defaults_and_pre18_not_null_do_not_occupy_constraint_names() -> testresult::TestResult {
    let context = test_helpers::postgres(
        "CREATE TABLE t(a integer CONSTRAINT same DEFAULT 1, b integer CONSTRAINT same DEFAULT 2, c integer CONSTRAINT same NOT NULL)",
    )?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(
        context.object_kind(&FullName::with_schema("public", "t")),
        Some(Kind::table())
    );
    Ok(())
}

#[test]
fn materialized_view_applies_the_output_column_name_list() -> testresult::TestResult {
    let context =
        test_helpers::postgres("CREATE MATERIALIZED VIEW v(renamed) AS SELECT 1 AS original")?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let view = context
        .get_view_impl(&FullName::with_schema("public", "v"))
        .expect("view");
    assert!(
        view.result_columns()
            .has_column(&ColumnName::from("renamed"))
    );
    assert!(
        !view
            .result_columns()
            .has_column(&ColumnName::from("original"))
    );
    Ok(())
}

#[test]
fn postgres18_named_not_null_occupies_the_constraint_namespace() -> testresult::TestResult {
    use crate::context::BaseParseContext;
    use crate::sem::{MutSourcePathWithSpan, SchemaFileContext, SchemaName};
    let mut metadata = ::sql_fun_core::SqlFunMetadataCollection::load_from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selected-postgres.toml"
    ))?;
    let selected = metadata
        .remove("analytics")
        .expect("PostgreSQL 18 metadata");
    let parent = test_helpers::postgres("")?.freeze();
    let mut context = SchemaFileContext::new(
        std::path::Path::new("not_null.sql"),
        &parent,
        parent.get_args(),
        &selected,
    );
    context.set_search_path(&[SchemaName::from("public")]);
    context.set_source_path_with_span(&crate::test_helpers::source_span());
    let context = test_helpers::apply(
        context,
        "CREATE TABLE t(a integer CONSTRAINT same NOT NULL, b integer CONSTRAINT same NOT NULL)",
    )?;
    assert_eq!(context.problems().len(), 1);
    assert_eq!(
        context.problems()[0]
            .creation_problem()
            .expect("conflict")
            .namespace(),
        "constraint"
    );
    assert!(
        context
            .object_kind(&FullName::with_schema("public", "t"))
            .is_none()
    );
    Ok(())
}
