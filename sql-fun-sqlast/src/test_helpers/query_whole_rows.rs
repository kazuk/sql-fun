use crate::StringSpan;
use crate::context::{MutSourcePathWithSpan, ParseContext as _, SchemaFileContext};
use crate::parse_statements;
use crate::sem::SemAst;
use crate::sem::convert;
use crate::sem::{AnalysisError, CastContext, CastDefinition, SourcePathWithSpan};
use crate::test_helpers::create_cache_context;
use crate::test_helpers::query::catalog;

use crate::ast::{Dialect, StatementNode};
use crate::names::FullName;
use crate::sem::{
    FunctionParam, OverloadVariant, PgBuiltInType, SelectStatement, SemScalarExpr, TypeReference,
};
use crate::test_helpers::TestParseContext;

pub(crate) fn assert_problem(context: &TestParseContext, code: &str) {
    for problem in context.reported_problems() {
        if problem.code() == code {
            return;
        }
    }
    panic!("missing {code}: {:?}", context.reported_problems());
}
pub(crate) fn row_levels(mut query: SelectStatement) -> Vec<usize> {
    #[derive(Default)]
    struct RowReferences(Vec<usize>);
    impl crate::sem::traversal::SemanticVisitorMut for RowReferences {
        fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if let Some(row) = value.as_whole_row() {
                self.0.push(row.levels_up().unwrap());
            }
            Ok(())
        }
    }
    let mut references = RowReferences::default();
    query.walk(&mut references).unwrap();
    references.0
}

pub(crate) fn view_query(sql: &str) -> (SelectStatement, SchemaFileContext) {
    schema_query("CREATE VIEW v AS SELECT 1 AS n", sql)
}
pub(crate) fn alias_query(sql: &str) -> (SelectStatement, SchemaFileContext) {
    schema_query(
        "CREATE VIEW v AS SELECT 1 AS n; CREATE TABLE t(n integer NOT NULL); CREATE VIEW w AS SELECT 1 AS n, 'a'::text AS label; CREATE VIEW nested AS SELECT identity_v(v.*) AS payload FROM v",
        sql,
    )
}
pub(crate) fn assert_catalog_view_field(context: &SchemaFileContext) {
    let ty = TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
    let columns = ty.row_columns(context).unwrap();
    assert!(
        columns
            .get_column_def(&crate::names::ColumnName::from("n"))
            .is_some()
    );
    assert!(
        columns
            .get_column_def(&crate::names::ColumnName::from("x"))
            .is_none()
    );
    assert!(
        columns
            .get_column_def(&crate::names::ColumnName::from("y"))
            .is_none()
    );
}
fn schema_query(ddl: &str, sql: &str) -> (SelectStatement, SchemaFileContext) {
    use crate::context::BaseParseContext;
    let mut cache_context = create_cache_context();
    let mut base = catalog();
    let nominal = TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
    base.set_get_implicit_cast_result(
        &nominal,
        &nominal,
        Some(CastDefinition::new(CastContext::no_conversion())),
    );
    base.setup_function(
        "identity_v",
        &[OverloadVariant::new(
            &Some(nominal.clone()),
            &vec![FunctionParam::new_input_param(&None, &Some(nominal), &None)],
            false,
            false,
        )],
    );
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    let mut context = SchemaFileContext::new(
        std::path::Path::new("whole_rows.sql"),
        &base.freeze(),
        &args,
        &metadata,
    );
    MutSourcePathWithSpan::set_source_path_with_span(
        &mut context,
        &SourcePathWithSpan::new("whole_rows.sql", &StringSpan::new(0, ddl.len())),
    );
    for statement in parse_statements(ddl, Dialect::postgres()) {
        context = SemAst::from_ast(&statement.unwrap(), context, &mut cache_context)
            .unwrap()
            .into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    // Exercise actual view application: a stub that only stores a ViewLike
    // would omit the nominal row type registered by SchemaCatalog::store_view.
    let row_type = TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
    assert!(row_type.row_columns(&context).is_some());
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let StatementNode::Query(query) = statement.into_test_node() else {
        panic!("query fixture");
    };
    context.begin_parameter_type_analysis();
    let result = convert(&query).resolve_test_in(&mut context, None);
    let result = context.infer_query_parameter_types(result, &mut create_cache_context());
    let query = context.finish_analysis(result).unwrap();
    (query, context)
}
pub(crate) fn catalog_query(sql: &str) -> (SelectStatement, TestParseContext) {
    let mut cache_context = create_cache_context();
    let mut context = catalog();
    for ddl in [
        "CREATE TABLE pg_catalog.pg_attribute(n integer)",
        "CREATE TABLE pg_catalog.pg_type(n integer)",
    ] {
        context = SemAst::from_ast(
            &parse_statements(ddl, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .unwrap()
        .into_context();
    }
    let mut inputs = Vec::new();
    for (name, parameter) in [("pg_attribute", "left_row"), ("pg_type", "right_row")] {
        let ty =
            TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", name), false);
        context.setup_type(ty.clone());
        inputs.push(FunctionParam::new_input_param(
            &Some(parameter.to_owned()),
            &Some(ty),
            &None,
        ));
    }
    context.setup_function(
        "type_arg",
        &[OverloadVariant::new(
            &Some(PgBuiltInType::int4()),
            &vec![FunctionParam::new_input_param(
                &None,
                &Some(TypeReference::concrete_type_ref(
                    &FullName::with_schema("pg_catalog", "pg_type"),
                    false,
                )),
                &None,
            )],
            false,
            false,
        )],
    );
    context.setup_type(TypeReference::from_master_type_name("oid"));
    context.setup_type(TypeReference::from_master_type_name("record"));
    context.setup_aggregate(
        "count",
        &[OverloadVariant::new(
            &Some(PgBuiltInType::int8()),
            &vec![],
            false,
            false,
        )],
    );
    context.setup_function(
        "accept_rows",
        &[OverloadVariant::new(
            &Some(TypeReference::from_master_type_name("oid")),
            &inputs,
            false,
            false,
        )],
    );
    context.setup_function(
        "record_arg",
        &[OverloadVariant::new(
            &Some(PgBuiltInType::int4()),
            &vec![FunctionParam::new_input_param(
                &None,
                &Some(TypeReference::from_master_type_name("record")),
                &None,
            )],
            false,
            false,
        )],
    );
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let StatementNode::Query(query) = statement.into_test_node() else {
        panic!("query fixture");
    };
    context.begin_parameter_type_analysis();
    let result = convert(&query).resolve_test_in(&mut context, None);
    let result = context.infer_query_parameter_types(result, &mut create_cache_context());
    let query = context.finish_analysis(result).unwrap();
    (query, context)
}
