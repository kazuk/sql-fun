use crate::{
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    names::{ColumnName, SchemaName},
    sem::{ColumnDefinition, PgBuiltInType, TypeCategory, TypeDefinition},
    test_helpers::{TestParseContext, source_span},
};

/// Three scalar identities and an explicit search path, without builtin metadata.
pub(crate) fn scalar_catalog(dialect: Dialect) -> TestParseContext {
    let mut base = TestParseContext::default().with_dialect(dialect);
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
    ] {
        base.setup_type(ty);
    }
    base
}

/// Scalar identities with their categories for common-type inference.
pub(crate) fn categorized_scalar_catalog(dialect: Dialect) -> TestParseContext {
    let mut base = scalar_catalog(dialect);
    for (ty, category) in [
        (PgBuiltInType::int4(), 'N'),
        (PgBuiltInType::bool(), 'B'),
        (PgBuiltInType::text(), 'S'),
    ] {
        let name = ty.full_name().expect("nominal type identity");
        base.set_get_type_impl_result(
            name,
            &TypeDefinition::builtin_with_category(
                name,
                Some(TypeCategory::try_from(category).unwrap()),
                None,
            ),
        );
    }
    base
}

/// The DML fixture's relation, retaining each caller's nullability contract.
pub(crate) fn items_catalog(dialect: Dialect, id_not_null: Option<bool>) -> TestParseContext {
    let mut context = scalar_catalog(dialect);
    context.set_source_path_with_span(&source_span());
    context.setup_table(
        "items",
        &[
            ColumnDefinition::new(
                &Some(ColumnName::from("id")),
                Some(&PgBuiltInType::int4()),
                id_not_null,
                &[],
            ),
            ColumnDefinition::new(
                &Some(ColumnName::from("label")),
                Some(&PgBuiltInType::text()),
                Some(false),
                &[],
            ),
        ],
        &source_span(),
    );
    context
}

/// Freeze the supplied test catalog without changing its analysis environment.
pub(crate) fn schema_from_base(
    base: TestParseContext,
    path: &std::path::Path,
) -> SchemaFileContext {
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(path, &base.freeze(), &args, &metadata)
}
