use crate::sem::{
    ColumnName, FullName, FunctionOverloadCollection, FunctionParam, FunctionSignatureMetadata,
    OverloadVariant, SchemaName, TypeReference,
};
use crate::types::{Candidates, ParameterMode, PolymorphicType, PseudoType, TypeContext};

mod test_helpers {
    use crate::ast::{StatementNode, TryFromWithTokens};
    use crate::sem::{FullName, FunctionOverloadCollection, PgBuiltInType, SchemaName, SemAst};
    use crate::test_helpers::{SynBuilder, TestParseContext};
    pub(super) fn context() -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
        context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::text());
        context
    }
    pub(super) fn declare(context: &mut TestParseContext, sql: &str) -> FunctionOverloadCollection {
        let syntax = StatementNode::try_from_with_tokens(
            &SynBuilder::new().parse(sql),
            &::sql_fun_core::IVec::default(),
        )
        .unwrap();
        let SemAst::CreateFunction(function) = syntax.resolve(context).unwrap() else {
            panic!("function fixture");
        };
        context.set_get_function_impl_result(function.name(), function.overloads());
        function.overloads().clone()
    }
    pub(super) fn name() -> FullName {
        FullName::with_schema("public", "f")
    }

    pub(super) mod heavy {
        use crate::sem::{BaseContext, FrozenBaseContext, SchemaFileContext};
        use ::clap::Parser;
        use ::sql_fun_core::SqlFunArgs;
        use std::path::{Path, PathBuf};
        /// Loads the real builtin catalog once for this caller's process. Required to
        /// verify extraction/ingestion fidelity, not just a synthetic signature.
        pub(in crate::types::compat::function) fn builtin_context() -> SchemaFileContext {
            let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
            let args = SqlFunArgs::try_parse_from([
                "sqlfun",
                "--metadata-file",
                metadata.to_str().unwrap(),
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
            ])
            .unwrap();
            let collection = args.load_metadata().unwrap();
            let selected = collection.get("default").unwrap();
            let base = FrozenBaseContext::from(BaseContext::new(&args, selected).unwrap());
            SchemaFileContext::new(Path::new("signatures.sql"), &base, &args, selected)
        }
    }
}

#[rstest::rstest]
#[case::scalar(
    "CREATE FUNCTION public.f(a int DEFAULT 7) RETURNS text LANGUAGE sql AS 'SELECT a::text'",
    false,
    false
)]
#[case::set(
    "CREATE FUNCTION public.f(a int) RETURNS SETOF text LANGUAGE sql AS 'SELECT a::text'",
    true,
    false
)]
#[case::table(
    "CREATE FUNCTION public.f(a int) RETURNS TABLE (b text) LANGUAGE sql AS 'SELECT a::text'",
    true,
    true
)]
fn common_ast_preserves_result_and_argument_metadata(
    #[case] sql: &str,
    #[case] set: bool,
    #[case] table: bool,
) {
    let mut context = test_helpers::context();
    let legacy = test_helpers::declare(&mut context, sql);
    let signatures = TypeContext::new(&context)
        .function_by_exact_name(&test_helpers::name())
        .unwrap()
        .unwrap();
    let signature = &signatures.signatures()[0];
    assert_eq!(signature.result().returns_set(), set);
    assert!(!signature.result().is_procedure());
    assert_eq!(
        signature.argument(0).unwrap().name(),
        Some(&ColumnName::new("a"))
    );
    assert_eq!(signature.parameters().len(), if table { 2 } else { 1 });
    if table {
        assert_eq!(signature.parameters()[1].mode(), ParameterMode::table());
    }
    if sql.contains("DEFAULT") {
        let default = signature.argument(0).unwrap().default_expression().unwrap();
        assert_eq!(default.representation(), "sql-fun-sem-expression-v1");
        assert_eq!(
            default.expression(),
            &serde_json::to_value(
                legacy.get_at(0).unwrap().params()[0]
                    .retained_default()
                    .unwrap()
            )
            .unwrap()
        );
    }
    assert_eq!(
        TypeContext::new(&context)
            .function_by_name(&FullName::from_local_name("f"))
            .unwrap(),
        Some(signatures)
    );
}

#[test]
fn unavailable_candidate_aborts_the_entire_overload_read() {
    let mut context = test_helpers::context();
    let mut overloads = test_helpers::declare(
        &mut context,
        "CREATE FUNCTION public.f(a int) RETURNS text LANGUAGE sql AS 'SELECT a::text'",
    );
    let incomplete = OverloadVariant::new(
        &Some(TypeReference::from_master_type_name("text")),
        &vec![],
        false,
        false,
    );
    overloads.push(&incomplete);
    context.set_get_function_impl_result(&test_helpers::name(), &overloads);
    let error = TypeContext::new(&context)
        .function_by_exact_name(&test_helpers::name())
        .unwrap_err();
    assert!(error.to_string().contains("overload 1, declaration"));
    assert!(
        TypeContext::new(&context)
            .function_by_name(&FullName::from_local_name("missing"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn polymorphic_arguments_and_results_share_one_signature_scope() {
    let mut context = test_helpers::context();
    let array = TypeReference::from_master_type_name("anyarray");
    let element = TypeReference::from_master_type_name("anyelement");
    let params = vec![FunctionParam::new_input_param(
        &Some("items".to_owned()),
        &Some(array),
        &None,
    )];
    let metadata = FunctionSignatureMetadata::new(&Some(element.clone()), &params, true, false);
    let overload = OverloadVariant::new(&Some(element), &params, false, false)
        .with_signature_metadata(&metadata);
    context.set_get_function_impl_result(
        &test_helpers::name(),
        &FunctionOverloadCollection::new(&[overload]),
    );
    let signatures = TypeContext::new(&context)
        .function_by_name(&FullName::from_local_name("f"))
        .unwrap()
        .unwrap();
    let s = &signatures.signatures()[0];
    let input = s
        .argument(0)
        .unwrap()
        .parameter_type()
        .as_any()
        .downcast_ref::<PolymorphicType>()
        .unwrap();
    let result = s
        .result()
        .result_type()
        .unwrap()
        .as_any()
        .downcast_ref::<PolymorphicType>()
        .unwrap();
    assert_eq!(input.variable().element(), *result.variable());
    assert_eq!(signatures.clone(), signatures);
}

#[test]
fn search_path_and_qualified_names_select_the_existing_catalog_definition() {
    let mut context = test_helpers::context();
    test_helpers::declare(
        &mut context,
        "CREATE FUNCTION public.f(a int) RETURNS text LANGUAGE sql AS 'SELECT a::text'",
    );
    test_helpers::declare(
        &mut context,
        "CREATE FUNCTION other.f(a text) RETURNS int LANGUAGE sql AS 'SELECT 1'",
    );
    context.set_get_search_path_result(&[
        SchemaName::from("other"),
        SchemaName::from("public"),
        SchemaName::from("pg_catalog"),
    ]);
    let loaded = TypeContext::new(&context);
    assert_eq!(
        loaded
            .function_by_name(&FullName::from_local_name("f"))
            .unwrap(),
        loaded
            .function_by_exact_name(&FullName::with_schema("other", "f"))
            .unwrap()
    );
    assert_ne!(
        loaded
            .function_by_name(&FullName::from_local_name("f"))
            .unwrap(),
        loaded.function_by_name(&test_helpers::name()).unwrap()
    );
}

#[test]
fn real_extracted_builtins_retain_scalar_set_record_and_polymorphic_forms() {
    let context = test_helpers::heavy::builtin_context();
    let loaded = TypeContext::new(&context);
    let scalar = loaded
        .function_by_name(&FullName::with_schema("pg_catalog", "json_to_record"))
        .unwrap()
        .unwrap();
    assert!(!scalar.signatures()[0].result().returns_set());
    assert!(
        scalar.signatures()[0]
            .result()
            .result_type()
            .unwrap()
            .as_any()
            .is::<PseudoType>()
    );
    let series = loaded
        .function_by_name(&FullName::with_schema("pg_catalog", "generate_series"))
        .unwrap()
        .unwrap();
    assert!(series.signatures().len() > 1);
    assert!(series.signatures().iter().all(|s| s.result().returns_set()));
    let keywords = loaded
        .function_by_name(&FullName::with_schema("pg_catalog", "pg_get_keywords"))
        .unwrap()
        .unwrap();
    assert!(keywords.signatures()[0].result().returns_set());
    assert!(!keywords.signatures()[0].parameters().is_empty());
    let defaults = loaded
        .function_by_name(&FullName::with_schema("pg_catalog", "jsonb_set_lax"))
        .unwrap()
        .unwrap();
    let signature = &defaults.signatures()[0];
    assert!(
        signature
            .argument(2)
            .unwrap()
            .default_expression()
            .is_none()
    );
    let first = signature.argument(3).unwrap().default_expression().unwrap();
    let second = signature.argument(4).unwrap().default_expression().unwrap();
    assert_eq!(first.representation(), "postgresql-sql-list");
    assert_eq!(first.index(), 0);
    assert_eq!(second.index(), 1);
    assert_eq!(first.expression(), second.expression());
    let unnest = loaded
        .function_by_name(&FullName::with_schema("pg_catalog", "unnest"))
        .unwrap()
        .unwrap();
    assert!(unnest.argument_candidates(0).as_any().is::<Candidates>());
}

#[rstest::rstest]
#[case::out(
    "CREATE FUNCTION public.f(OUT a int, OUT b text) LANGUAGE sql AS 'SELECT 1, ''x'''",
    false,
    ParameterMode::output()
)]
#[case::in_out(
    "CREATE FUNCTION public.f(INOUT a int DEFAULT 1) LANGUAGE sql AS 'SELECT a'",
    false,
    ParameterMode::in_out()
)]
#[case::procedure(
    "CREATE PROCEDURE public.f(INOUT a int) LANGUAGE sql AS 'SELECT a'",
    true,
    ParameterMode::in_out()
)]
#[case::variadic_default(
    "CREATE FUNCTION public.f(VARIADIC a int[] DEFAULT NULL) RETURNS int LANGUAGE sql AS 'SELECT 1'",
    false,
    ParameterMode::variadic()
)]
#[case::quoted(
    r#"CREATE FUNCTION public.f("Case" int) RETURNS int LANGUAGE sql AS 'SELECT 1'"#,
    false,
    ParameterMode::input()
)]
fn common_ast_retains_out_inout_procedure_and_quoted_names(
    #[case] sql: &str,
    #[case] procedure: bool,
    #[case] mode: ParameterMode,
) {
    let mut context = test_helpers::context();
    test_helpers::declare(&mut context, sql);
    let signatures = TypeContext::new(&context)
        .function_by_exact_name(&test_helpers::name())
        .unwrap()
        .unwrap();
    let signature = &signatures.signatures()[0];
    assert_eq!(signature.result().is_procedure(), procedure);
    assert_eq!(signature.parameters()[0].mode(), mode);
    if sql.contains("DEFAULT") {
        assert!(signature.parameters()[0].default_expression().is_some());
    }
    if sql.contains("Case") {
        assert!(
            signature
                .argument_by_name(&ColumnName::new("Case"))
                .unwrap()
                .is_some()
        );
        assert!(
            signature
                .argument_by_name(&ColumnName::new("case"))
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn common_ast_catalog_storage_and_merging_preserve_complete_signatures() {
    use crate::ast::{StatementNode, TryFromWithTokens};
    use crate::test_helpers::SynBuilder;
    let mut catalog = test_helpers::heavy::builtin_context();
    for sql in [
        "CREATE FUNCTION public.signature_storage(a int DEFAULT 1) RETURNS text LANGUAGE sql AS 'SELECT a::text'",
        "CREATE FUNCTION public.signature_storage(a text) RETURNS SETOF int LANGUAGE sql AS 'SELECT 1'",
    ] {
        let syntax = StatementNode::try_from_with_tokens(
            &SynBuilder::new().parse(sql),
            &::sql_fun_core::IVec::default(),
        )
        .unwrap();
        catalog = syntax.resolve_and_apply(catalog).unwrap().into_context();
    }
    let signatures = TypeContext::new(&catalog)
        .function_by_exact_name(&FullName::with_schema("public", "signature_storage"))
        .unwrap()
        .unwrap();
    assert_eq!(signatures.signatures().len(), 2);
    assert_eq!(
        signatures
            .signatures()
            .iter()
            .filter(|s| s.result().returns_set())
            .count(),
        1
    );
    assert_eq!(
        signatures
            .signatures()
            .iter()
            .filter(|s| s.argument(0).unwrap().default_expression().is_some())
            .count(),
        1
    );
}

#[test]
fn known_record_results_project_complete_structural_output_members() {
    let mut context = test_helpers::context();
    test_helpers::declare(
        &mut context,
        "CREATE FUNCTION public.f(OUT a int, OUT b text) LANGUAGE sql AS 'SELECT 1, ''x'''",
    );
    let signatures = TypeContext::new(&context)
        .function_by_exact_name(&test_helpers::name())
        .unwrap()
        .unwrap();
    let result = signatures.signatures()[0].result();
    assert!(result.declared_type().unwrap().as_any().is::<PseudoType>());
    let row = result
        .result_type()
        .unwrap()
        .as_any()
        .downcast_ref::<crate::types::ImplicitCompositeType>()
        .unwrap();
    assert_eq!(row.members().members().len(), 2);
    assert_eq!(
        row.members().members()[0].name(),
        Some(&ColumnName::new("a"))
    );
    assert_eq!(
        signatures.filter_result(result.result_type().unwrap()),
        signatures
    );
}
