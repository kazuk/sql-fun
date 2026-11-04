use crate::AstAndContextPair;
use crate::ast::{Dialect, StatementNode};
use crate::context::{MutSourcePathWithSpan as _, ParseContext as _};
use crate::names::{ColumnName, FullName, OperatorName, SchemaName, TableName};
use crate::parse_statements;
use crate::sem::{
    AnalysisError, BinaryOperatorDefinition, CastContext, CastDefinition, CatalogNameLookup,
    CompositeFieldCollection, CompositeFieldDefinition, CreateCompositeType, CreateView,
    FunctionParam, OperatorDefinition, OverloadVariant, PgBuiltInType, SelectStatement,
    SemScalarExpr, TypeCategory, TypeDefinition, TypeReference, ViewLike,
};
use crate::test_helpers::{TestParseContext, create_cache_context, source_span};

pub(crate) fn catalog() -> TestParseContext {
    let mut context = TestParseContext::default();
    context.set_source_path_with_span(&source_span());
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int2(),
        PgBuiltInType::int4(),
        PgBuiltInType::int4().array_type(),
        PgBuiltInType::int8(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
        PgBuiltInType::numeric(),
        TypeReference::from_master_type_name("record"),
        TypeReference::from_master_type_name("interval"),
        TypeReference::from_master_type_name("date"),
    ] {
        context.setup_type(ty.clone());
        context.set_get_implicit_cast_result(
            &ty,
            &ty,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
    }
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int4(),
        &PgBuiltInType::int8(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int2(),
        &PgBuiltInType::int4(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let int = PgBuiltInType::int4();
    let add = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(&int, &int, &int, true)]);
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "+"),
        true,
        true,
        Some(&add),
    );
    let comparison = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
        &int,
        &int,
        &PgBuiltInType::bool(),
        true,
    )]);
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "<"),
        true,
        true,
        Some(&comparison),
    );
    let sum = OverloadVariant::new(
        &Some(int.clone()),
        &vec![FunctionParam::new_input_param(&None, &Some(int), &None)],
        false,
        false,
    );
    context.setup_function("identity", std::slice::from_ref(&sum));
    context.setup_aggregate("sum", &[sum]);
    context.setup_aggregate(
        "mode",
        &[OverloadVariant::new(
            &Some(PgBuiltInType::int4()),
            &vec![],
            false,
            false,
        )],
    );
    context
}

pub(crate) fn resolve(
    sql: &str,
    dialect: Dialect,
    infer: bool,
) -> Result<(SelectStatement, TestParseContext), AnalysisError> {
    let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
    let StatementNode::Query(query) = statement.into_test_node() else {
        panic!("query fixture expected");
    };
    let mut catalog = catalog().with_dialect(dialect);
    catalog.begin_parameter_type_analysis();
    let query = query.resolve_test_in(&mut catalog, None);
    let query = if infer {
        catalog.infer_query_parameter_types(query, &mut create_cache_context())
    } else {
        query
    };
    let query = catalog.finish_analysis(query)?;
    Ok((query, catalog))
}

pub(crate) fn query(sql: &str) -> SelectStatement {
    resolve(sql, Dialect::postgres(), false).unwrap().0
}

pub(crate) fn output_index(expression: &SemScalarExpr) -> Option<usize> {
    expression.output_index()
}

pub(crate) fn output_type(query: &SelectStatement, name: &str) -> Option<TypeReference> {
    query
        .get_result_column_def(&ColumnName::from(name))
        .unwrap()
        .get_type_cloned()
}

/// Small explicitly selected table for grouping placement checks in DML.
pub(crate) fn grouping_table_context() -> TestParseContext {
    let mut cache_context = create_cache_context();
    parse_statements("CREATE TABLE t(a integer)", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .analyze(
            catalog().with_dialect(Dialect::postgres()),
            &mut cache_context,
        )
        .unwrap()
        .into_context()
}

/// Resolve a query against explicitly selected, minimal column-scope fixtures.
pub(crate) fn column_scope_query(
    sql: &str,
    dialect: Dialect,
) -> (SelectStatement, TestParseContext) {
    let mut cache_context = create_cache_context();
    let mut context = catalog().with_dialect(dialect);
    for ddl in [
        "CREATE TABLE outer_items(id integer)",
        "CREATE TABLE local_items(other integer)",
        "CREATE TABLE known_items(id integer)",
    ] {
        context = parse_statements(ddl, dialect)
            .next()
            .unwrap()
            .unwrap()
            .analyze(context, &mut cache_context)
            .unwrap()
            .into_context();
    }
    let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
    let StatementNode::Query(query) = statement.into_test_node() else {
        panic!("query fixture");
    };
    let query = query.resolve_test_in(&mut context, None).unwrap();
    (query, context)
}

/// Analyze against explicitly supplied, small cross-schema relation fixtures.
pub(crate) fn source_namespace_statement(sql: &str) -> AstAndContextPair<TestParseContext> {
    source_namespace_statement_at_version(sql, "17")
}

pub(crate) fn source_namespace_statement_at_version(
    sql: &str,
    version: &str,
) -> AstAndContextPair<TestParseContext> {
    let mut cache_context = create_cache_context();
    let dialect = Dialect::postgres();
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["a", "public"]
    })).unwrap();
    let mut context = catalog().with_metadata(&metadata);
    // Merged FULL JOIN keys use PostgreSQL's common-type selection.
    for ty in [PgBuiltInType::int4(), PgBuiltInType::int8()] {
        context.set_get_type_impl_result(
            ty.full_name().expect("nominal type identity"),
            &TypeDefinition::builtin_with_category(
                ty.full_name().expect("nominal type identity"),
                Some(TypeCategory::try_from('N').unwrap()),
                None,
            ),
        );
    }
    context.set_get_search_path_result(&[SchemaName::from("a"), SchemaName::from("public")]);
    let schema = r#"
CREATE TABLE a.t(id integer NOT NULL, only_a integer NOT NULL);
CREATE TABLE a.zero_columns();
CREATE TABLE a.mixed(id integer NOT NULL, other bigint NOT NULL);
CREATE TABLE b.t(id bigint NOT NULL, only_b bigint NOT NULL);
CREATE TABLE a.u(aid integer NOT NULL);
CREATE TABLE b.u(bid bigint NOT NULL);
CREATE TABLE "a.b"."t.x"(id integer NOT NULL);
CREATE TABLE "b.b"."t.x"(id bigint NOT NULL);
"#;
    for statement in parse_statements(schema, dialect) {
        context = statement
            .unwrap()
            .analyze(context, &mut cache_context)
            .unwrap()
            .into_context();
    }
    let mixed = FullName::with_schema("a", "mixed");
    context.set_get_type_impl_result(
        &mixed,
        &TypeDefinition::table_ref(&TableName::from(mixed.clone())),
    );
    context.setup_function(
        "mixed_fun",
        &[OverloadVariant::new(
            &Some(TypeReference::from_full_name(mixed)),
            &vec![],
            false,
            false,
        )],
    );
    for (schema, name, source, projection) in [
        ("a", "v", "a.t", "id"),
        ("b", "v", "b.t", "id"),
        ("c", "t", "b.t", "id"),
        ("a", "mixed_v", "a.mixed", "id, other"),
    ] {
        let sql = format!("SELECT {projection} FROM {source}");
        let query = parse_statements(&sql, dialect)
            .next()
            .unwrap()
            .unwrap()
            .into_test_node()
            .resolve(&mut context, &mut create_cache_context())
            .unwrap()
            .into_select_statemtnt()
            .unwrap();
        let name = FullName::with_schema(schema, name);
        let view = CreateView::new(&name, &query, &[], &None, false);
        context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    }
    assert!(
        context.reported_problems().is_empty(),
        "{:?}",
        context.reported_problems()
    );
    parse_statements(sql, dialect)
        .next()
        .unwrap()
        .unwrap()
        .analyze(context, &mut cache_context)
        .unwrap()
}

/// Analyze using only the signatures/types needed by record-output tests.
pub(crate) fn table_function_query(sql: &str, infer: bool) -> (SelectStatement, TestParseContext) {
    let mut cache_context = create_cache_context();
    let dialect = Dialect::postgres();
    let mut context = catalog();
    for name in [
        "varchar",
        "bpchar",
        "bit",
        "varbit",
        "timestamp",
        "timestamptz",
        "time",
        "timetz",
    ] {
        context.setup_type(TypeReference::from_master_type_name(name));
        context.setup_type(TypeReference::from_master_type_name(name).array_type());
    }
    context.setup_type(TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "custom_type"),
        false,
    ));
    let record = TypeReference::from_master_type_name("record");
    context.setup_type(record.clone());
    context.setup_type(PgBuiltInType::text().array_type());
    let input = FunctionParam::new_input_param(
        &Some("src".to_owned()),
        &Some(PgBuiltInType::text()),
        &None,
    );
    let overload = OverloadVariant::new(&Some(record.clone()), &vec![input.clone()], false, false);
    context.setup_function("record_fun", std::slice::from_ref(&overload));
    let integer_overload = OverloadVariant::new(
        &Some(record.clone()),
        &vec![FunctionParam::new_input_param(
            &Some("src".to_owned()),
            &Some(PgBuiltInType::int4()),
            &None,
        )],
        false,
        false,
    );
    context.setup_function("overloaded_record", &[overload, integer_overload]);
    let collation_name = FullName::from_local_name("C");
    context.set_collation_lookup(
        &collation_name,
        &CatalogNameLookup::found(&collation_name, &FullName::with_schema("pg_catalog", "C")),
    );
    context.setup_function(
        "out_fun",
        &[OverloadVariant::new(
            &Some(record),
            &vec![
                FunctionParam::new_table_param(&Some("a".to_owned()), &Some(PgBuiltInType::int4())),
                FunctionParam::new_table_param(&Some("b".to_owned()), &Some(PgBuiltInType::text())),
            ],
            false,
            false,
        )],
    );
    let name = FullName::with_schema("public", "pair");
    let mut fields = CompositeFieldCollection::default();
    fields.push(CompositeFieldDefinition::new(
        "a",
        &Some(PgBuiltInType::int4()),
        &None,
    ));
    fields.push(CompositeFieldDefinition::new(
        "b",
        &Some(PgBuiltInType::text()),
        &None,
    ));
    let definition = CreateCompositeType::new(&name, &fields);
    context.set_get_type_impl_result(&name, &TypeDefinition::from(&definition));
    let pair = TypeReference::concrete_type_ref(&name, false);
    context.setup_function(
        "pair_fun",
        &[OverloadVariant::new(&Some(pair), &vec![], false, false)],
    );
    context = parse_statements("CREATE TABLE public.row_item(a integer NOT NULL)", dialect)
        .next()
        .unwrap()
        .unwrap()
        .analyze(context, &mut cache_context)
        .unwrap()
        .into_context();
    let row_name = FullName::with_schema("public", "row_item");
    context.set_get_type_impl_result(
        &row_name,
        &TypeDefinition::table_ref(&TableName::from(row_name.clone())),
    );
    context.setup_function(
        "row_fun",
        &[OverloadVariant::new(
            &Some(TypeReference::concrete_type_ref(&row_name, false)),
            &vec![],
            false,
            false,
        )],
    );
    let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
    let StatementNode::Query(query) = statement.into_test_node() else {
        panic!("query fixture");
    };
    context.begin_parameter_type_analysis();
    let query = query.resolve_test_in(&mut context, None);
    let query = if infer {
        context.infer_query_parameter_types(query, &mut create_cache_context())
    } else {
        query
    };
    (context.finish_analysis(query).unwrap(), context)
}

pub(crate) fn expression(query: &SelectStatement) -> &SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}
