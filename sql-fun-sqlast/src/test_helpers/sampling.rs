use crate::{
    ast::Dialect,
    context::ParseContext,
    sem::{
        CastContext, CastDefinition, ColumnDefinition, ColumnName, FullName, PgBuiltInType,
        SamplingMethod, SamplingMethodLookup, SchemaName, SelectStatement, TableSampleSource,
    },
    test_helpers::{TestParseContext, source_span},
};

/// Small explicit offline catalog. No real builtin schema is loaded.
pub(crate) fn context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::postgres());
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    for ty in [
        PgBuiltInType::int4(),
        crate::sem::TypeReference::from_master_type_name("float4"),
        crate::sem::TypeReference::from_master_type_name("float8"),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
    ] {
        context.setup_type(ty);
    }
    for target in [
        crate::sem::TypeReference::from_master_type_name("float4"),
        crate::sem::TypeReference::from_master_type_name("float8"),
    ] {
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &target,
            Some(CastDefinition::new(CastContext::Implicit)),
        );
        context.add_implicit_castable(&PgBuiltInType::int4(), &target);
    }
    context.setup_table(
        "sampled",
        &[
            ColumnDefinition::new(
                &Some(ColumnName::from("id")),
                Some(&PgBuiltInType::int4()),
                Some(true),
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
    for name in ["system", "bernoulli"] {
        let name = FullName::with_schema("pg_catalog", name);
        context.set_sampling_method(
            &name,
            &SamplingMethodLookup::found(&SamplingMethod::new(
                &name,
                &[crate::sem::TypeReference::from_master_type_name("float4")],
                true,
            )),
        );
    }
    let custom = FullName::with_schema("public", "custom_sample");
    context.set_sampling_method(
        &custom,
        &SamplingMethodLookup::found(&SamplingMethod::new(
            &custom,
            &[PgBuiltInType::int4(), PgBuiltInType::text()],
            false,
        )),
    );
    for (name, preserves) in [
        ("scalar507", true),
        ("aggregate507", false),
        ("set507", false),
    ] {
        let overload = crate::sem::OverloadVariant::new(
            &Some(PgBuiltInType::int4()),
            &vec![crate::sem::FunctionParam::new_input_param(
                &None,
                &Some(PgBuiltInType::int4()),
                &None,
            )],
            false,
            true,
        );
        let identity = if name == "aggregate507" {
            context.setup_aggregate(name, std::slice::from_ref(&overload));
            FullName::with_schema("public", name)
        } else {
            context.setup_function(name, std::slice::from_ref(&overload))
        };
        context.set_function_row_property(&identity, &overload, preserves);
        context.set_function_returns_set(&identity, &overload, name == "set507");
    }
    context
}

pub(crate) use crate::test_helpers::collate::query;

pub(crate) fn sources(query: &SelectStatement) -> Vec<TableSampleSource> {
    let mut collector = Samples::default();
    query.clone().walk(&mut collector).unwrap();
    collector.0
}
#[derive(Default)]
struct Samples(Vec<TableSampleSource>);
impl crate::sem::traversal::SemanticVisitorMut for Samples {
    fn table_sample(
        &mut self,
        value: &mut TableSampleSource,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.0.push(value.clone());
        Ok(())
    }
}

/// Explicit view definitions sharing the same analyzed output contract.
pub(crate) fn with_views(mut context: TestParseContext) -> TestParseContext {
    let query = query("SELECT id,label FROM sampled", &mut context).unwrap();
    let ordinary = FullName::with_schema("public", "ordinary507");
    let view = crate::sem::CreateView::new(&ordinary, &query, &[], &None, false);
    context.set_get_view_impl_result(&ordinary, &Some(crate::sem::ViewLike::from(&view)));
    let materialized = FullName::with_schema("public", "materialized507");
    let definition =
        crate::sem::CreateTableAs::new(&materialized, &crate::sem::SemAst::select(&query), true);
    context.set_get_view_impl_result(
        &materialized,
        &Some(crate::sem::ViewLike::from(&definition)),
    );
    context
}

/// Explicit schema layer for function-property tests; no builtin initialization.
pub(crate) fn schema_context() -> crate::context::SchemaFileContext {
    let base = context().freeze();
    crate::test_helpers::schema_layer(std::path::Path::new("sampling-properties.sql"), &base)
}
