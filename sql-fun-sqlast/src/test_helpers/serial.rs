//! Minimal catalog for serial declaration and schema transition tests.
use crate::{
    context::{MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    names::{FullName, SchemaName},
    sem::{
        CastContext, CastDefinition, ColumnConstraint, ColumnDefinition, DefaultColumnConstraint,
        FunctionOverloadCollection, FunctionParam, OverloadVariant, PgBuiltInType, SemScalarExpr,
        TypeReference,
    },
    test_helpers::{TestParseContext, schema_layer, source_span},
};

pub(crate) fn context() -> SchemaFileContext {
    let mut base = TestParseContext::default();
    base.set_search_path(&[SchemaName::from("public")]);
    for ty in [
        TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int2")),
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
        TypeReference::from_full_name(FullName::with_schema("pg_catalog", "regclass")),
    ] {
        base.setup_type(ty);
    }
    base.set_get_implicit_cast_result(
        &PgBuiltInType::text(),
        &TypeReference::from_full_name(FullName::with_schema("pg_catalog", "regclass")),
        Some(CastDefinition::new(CastContext::explicit())),
    );
    let overload = OverloadVariant::new(
        &Some(PgBuiltInType::int8()),
        &vec![FunctionParam::new_input_param(
            &None,
            &Some(TypeReference::from_full_name(FullName::with_schema(
                "pg_catalog",
                "regclass",
            ))),
            &None,
        )],
        true,
        false,
    );
    base.set_get_function_impl_result(
        &FullName::with_schema("pg_catalog", "nextval"),
        &FunctionOverloadCollection::new(&[overload]),
    );
    let mut context = schema_layer(std::path::Path::new("serial.sql"), &base.freeze());
    context.set_source_path_with_span(&source_span());
    context
}

impl ColumnDefinition {
    pub(crate) fn serial_default_expression(&self) -> Option<&SemScalarExpr> {
        self.constraints()
            .iter()
            .find_map(ColumnConstraint::as_default)
            .map(DefaultColumnConstraint::expr)
    }
}
