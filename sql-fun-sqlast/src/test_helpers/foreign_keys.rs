//! Small explicit catalog and SQL fixtures for foreign-key binding contracts.
use crate::{
    context::{
        BaseParseContext, FrozenBaseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext,
    },
    sem::{ColumnName, ForeignKeyColumnConstraint, FullName, SchemaName, TableName},
    test_helpers::{TestParseContext, ownership::apply_ownership_sql, source_span},
};

pub(crate) fn context() -> SchemaFileContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine":"postgres", "dialect":"postgres", "version":"17", "search_path":["a","b"]
    }))
    .unwrap();
    let mut base = TestParseContext::default().with_metadata(&metadata);
    base.set_search_path(&[SchemaName::from("a"), SchemaName::from("b")]);
    base.setup_type(crate::sem::PgBuiltInType::int4());
    base.setup_type(crate::sem::PgBuiltInType::text());
    child(&base.freeze())
}

pub(crate) fn child(base: &FrozenBaseContext) -> SchemaFileContext {
    let mut context = SchemaFileContext::new(
        std::path::Path::new("foreign-keys.sql"),
        base,
        base.get_args(),
        base.metadata(),
    );
    context.set_source_path_with_span(&source_span());
    context
}

pub(crate) fn apply(context: SchemaFileContext, sql: &str) -> SchemaFileContext {
    apply_ownership_sql(context, sql)
        .expect("analyze SQL")
        .into_context()
}

pub(crate) fn parents() -> SchemaFileContext {
    let context = apply(
        context(),
        "CREATE TABLE a.parent(id integer CONSTRAINT parent_pk PRIMARY KEY)",
    );
    apply(
        context,
        "CREATE TABLE b.parent(id integer CONSTRAINT parent_pk PRIMARY KEY)",
    )
}

pub(crate) fn bind(
    context: SchemaFileContext,
    child: &str,
    reference: &str,
    table_constraint: bool,
) -> SchemaFileContext {
    if table_constraint {
        let context = apply(context, &format!("CREATE TABLE a.{child}(id integer)"));
        apply(
            context,
            &format!(
                "ALTER TABLE a.{child} ADD CONSTRAINT fk FOREIGN KEY(id) REFERENCES {reference}"
            ),
        )
    } else {
        apply(
            context,
            &format!("CREATE TABLE a.{child}(id integer REFERENCES {reference})"),
        )
    }
}

pub(crate) fn inline(
    context: &impl BaseParseContext,
    child: &FullName,
) -> ForeignKeyColumnConstraint {
    let definition = context.get_table_impl(child).expect("child table");
    let column = definition
        .get_column_def(&ColumnName::from("id"))
        .expect("child column");
    for constraint in column.constraints() {
        if let Some(fk) = constraint.as_foreign_key() {
            return fk.clone();
        }
    }
    panic!("inline FK");
}

pub(crate) fn target(
    context: &impl BaseParseContext,
    child: &str,
    table_constraint: bool,
) -> Option<TableName> {
    let child = FullName::with_schema("a", child);
    if table_constraint {
        context
            .get_table_impl(&child)
            .expect("child table")
            .constraint("fk")
            .expect("table constraint")
            .as_foreign_key()
            .expect("table FK")
            .target_table()
            .cloned()
    } else {
        inline(context, &child).primary_key_table().cloned()
    }
}

pub(crate) fn reversed_path(context: &mut impl ParseContext) {
    context.set_search_path(&[SchemaName::from("b"), SchemaName::from("a")]);
}

pub(crate) fn name(schema: &str, local: &str) -> TableName {
    TableName::from(FullName::with_schema(schema, local))
}

pub(crate) fn postgres_constraint(sql: &str) -> crate::syn::Constraint {
    use crate::syn::{ListOpt, Opt};
    let node = crate::test_helpers::SynBuilder::new().parse(sql);
    let alter = node.as_alter_table_stmt().as_inner().expect("ALTER TABLE");
    let commands = alter.get_cmds().as_inner().expect("commands");
    commands[0]
        .as_alter_table_cmd()
        .get_def()
        .as_constraint()
        .as_inner()
        .expect("constraint")
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> SchemaFileContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine":"SQLite", "dialect":"SQLite", "version":"3", "search_path":["main"]
    }))
    .unwrap();
    let mut base = TestParseContext::default()
        .with_metadata(&metadata)
        .with_dialect(crate::ast::Dialect::sqlite());
    base.set_search_path(&[SchemaName::from("main")]);
    child(&base.freeze())
}

#[cfg(feature = "sqlite")]
pub(crate) fn apply_sqlite(context: SchemaFileContext, sql: &str) -> SchemaFileContext {
    let mut cache_context = crate::CacheContext::new();
    crate::parse_statements(sql, crate::ast::Dialect::sqlite())
        .next()
        .unwrap()
        .expect("SQLite SQL")
        .analyze(context, &mut cache_context)
        .expect("SQLite analysis")
        .into_context()
}
