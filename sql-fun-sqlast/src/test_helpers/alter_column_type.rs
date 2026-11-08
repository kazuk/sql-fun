//! Explicit small catalog for analysis-only ALTER COLUMN contracts.

use crate::{
    ast::Dialect,
    context::{MutSourcePathWithSpan, ParseContext},
    names::{ColumnName, FullName, SchemaName},
    sem::{
        AnalysisError, CastContext, CastDefinition, CatalogNameLookup, ColumnDefinition,
        PgBuiltInType, TypeDefinition,
    },
    test_helpers::{TestParseContext, source_span},
};

pub(crate) fn context() -> TestParseContext {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "PostgreSQL", "dialect": "postgres", "version": "17",
        "search_path": ["public", "pg_catalog"]
    }))
    .unwrap();
    let mut context = TestParseContext::default()
        .with_metadata(&metadata)
        .with_dialect(Dialect::postgres());
    context.set_search_path(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    context.set_source_path_with_span(&source_span());
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
        PgBuiltInType::text(),
        PgBuiltInType::bool(),
        PgBuiltInType::numeric(),
    ] {
        context.setup_type(ty.clone());
        let name = ty.full_name().expect("nominal fixture type");
        context.set_get_type_impl_result(
            name,
            &TypeDefinition::builtin(name, Some(ty == PgBuiltInType::text())),
        );
    }
    let name = FullName::from_local_name("C");
    let resolved = FullName::with_schema("pg_catalog", "C");
    context.set_collation_lookup(&name, &CatalogNameLookup::found(&name, &resolved));
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int4(),
        &PgBuiltInType::int8(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    context.setup_table(
        "t",
        &[
            ColumnDefinition::new(
                &Some(ColumnName::from("a")),
                Some(&PgBuiltInType::int4()),
                Some(false),
                &[],
            ),
            ColumnDefinition::new(
                &Some(ColumnName::from("b")),
                Some(&PgBuiltInType::int4()),
                Some(true),
                &[],
            ),
        ],
        &source_span(),
    );
    context
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> TestParseContext {
    use crate::sem::TypeReference;

    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "SQLite", "dialect": "SQLite", "version": "3.51",
        "search_path": ["public"]
    }))
    .unwrap();
    let mut context = context()
        .with_metadata(&metadata)
        .with_dialect(Dialect::sqlite());
    for name in ["integer", "text"] {
        context.setup_type(TypeReference::concrete_type_ref(
            &FullName::with_schema("public", name),
            false,
        ));
    }
    context
}

pub(crate) fn with_default() -> TestParseContext {
    use crate::{
        names::TableName,
        sem::{ColumnConstraint, DefaultColumnConstraint, ScalarConstExpr, SemScalarExpr},
    };

    let mut context = context();
    let column = ColumnName::from("a");
    let default = DefaultColumnConstraint::new(
        &TableName::from(FullName::with_schema("public", "t")),
        &column,
        SemScalarExpr::new_const(ScalarConstExpr::string("1")),
    );
    context.setup_table(
        "t",
        &[
            ColumnDefinition::new(
                &Some(column),
                Some(&PgBuiltInType::int4()),
                Some(false),
                &[ColumnConstraint::default(default)],
            ),
            ColumnDefinition::new(
                &Some(ColumnName::from("b")),
                Some(&PgBuiltInType::int4()),
                Some(true),
                &[],
            ),
        ],
        &source_span(),
    );
    context
}

pub(crate) fn declared_default(default: &str) -> Result<TestParseContext, AnalysisError> {
    use crate::{
        sem::SemAst,
        test_helpers::{create_cache_context, public_analysis},
    };

    let statement = public_analysis::statement(
        &format!("CREATE TABLE defaults(a text DEFAULT ({default}))"),
        Dialect::postgres(),
    );
    Ok(SemAst::from_ast(&statement, context(), &mut create_cache_context())?.into_context())
}
