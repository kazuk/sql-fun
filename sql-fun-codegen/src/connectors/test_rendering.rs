//! Connector API tests supplement the executable macro fixtures.

use super::ConnectorSpec;
use crate::test_helpers::{
    GeneratedFunction, RenderFunction, assert_stub, lockfile_entry, query_spec,
};
#[cfg(feature = "sqlite")]
use ::sql_fun_core::SqlDialect;

#[rstest::rstest]
#[case::one(ConnectorSpec::render_query_one, "Row", "query_one")]
#[case::optional(ConnectorSpec::render_query_opt, "Option<Row>", "query_opt")]
#[case::many(ConnectorSpec::render_query_many, "Vec<Row>", "query_raw")]
#[case::execute(ConnectorSpec::render_execute, "u64", "execute")]
fn renders_postgres_operations(
    #[case] render: RenderFunction,
    #[case] row_type: &str,
    #[case] method: &str,
    #[values(false, true)] interrupted: bool,
) {
    let return_type: syn::Type = syn::parse_str(row_type).unwrap();
    let mut spec = query_spec(syn::parse_quote! {
        async fn generated<F: Fn(Vec<Row>, Row) -> Fut>(connection: Client, id: i64, #[parameter_source] dto: Dto, mut collector: Vec<Row>, handler: F) -> Result<#return_type, QueryError> {}
    });
    spec.result_type.return_type = return_type;
    spec.lockfile_entry = Some(lockfile_entry(
        "opaque SQL",
        &[("name", 0), ("id", 1)],
        &[("id", 3), ("name", 1)],
    ));
    if interrupted {
        spec.interrupt_handler = Some(syn::parse_quote!(interrupt));
    }
    let function: syn::ItemFn = syn::parse2(render(&spec.connector_spec, &spec).unwrap()).unwrap();
    assert_eq!(function.sig, spec.item_fn.codegen_signature());
    let observation = GeneratedFunction::inspect(&function);
    observation.assert_binding_array(&[syn::parse_quote!(&dto.name()), syn::parse_quote!(&id)]);
    assert_eq!(observation.has_macro("select"), interrupted);
    if row_type != "u64" {
        observation.assert_method("try_get", &[syn::parse_quote!(3)]);
        observation.assert_method("try_get", &[syn::parse_quote!(1)]);
        observation.assert_method("id", &[syn::parse_quote!(__sql_fun_result.try_get(3)?)]);
        observation.assert_method("name", &[syn::parse_quote!(__sql_fun_result.try_get(1)?)]);
        observation.assert_method("build", &[]);
    }
    if !interrupted && method != "query_raw" {
        // The driver call is an ordinary AST node; select! bodies are validated by runtime fixtures.
        let params: syn::Expr = syn::parse_quote!(__sql_fun_params);
        observation.assert_method(method, &[syn::parse_quote!("opaque SQL"), params]);
    }
    assert!(!observation.has_macro("todo"));
    assert!(observation.has_macro("file"));
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::one(ConnectorSpec::render_query_one, "Row")]
#[case::optional(ConnectorSpec::render_query_opt, "Option<Row>")]
#[case::many(ConnectorSpec::render_query_many, "Vec<Row>")]
#[case::execute(ConnectorSpec::render_execute, "usize")]
fn renders_sqlite_operations(
    #[case] render: RenderFunction,
    #[case] row_type: &str,
    #[values(false, true)] interrupted: bool,
) {
    let return_type: syn::Type = syn::parse_str(row_type).unwrap();
    let mut spec = query_spec(syn::parse_quote! {
        fn generated<F: Fn(Vec<Row>, Row) -> Result<Vec<Row>, QueryError>>(connection: Client, id: i64, mut collector: Vec<Row>, handler: F) -> Result<#return_type, QueryError> {}
    });
    spec.connector_spec = ConnectorSpec::realize(SqlDialect::SQLite, "rusqlite").unwrap();
    spec.result_type.return_type = return_type;
    spec.lockfile_entry = Some(lockfile_entry("opaque SQL", &[("id", 0)], &[("id", 3)]));
    if interrupted {
        spec.interrupt_handler = Some(syn::parse_quote!(interrupt));
    }
    let function: syn::ItemFn = syn::parse2(render(&spec.connector_spec, &spec).unwrap()).unwrap();
    let observation = GeneratedFunction::inspect(&function);
    observation.assert_binding_array(&[syn::parse_quote!(&id)]);
    if row_type == "usize" {
        observation.assert_method(
            "execute",
            &[
                syn::parse_quote!("opaque SQL"),
                syn::parse_quote!(__sql_fun_params),
            ],
        );
    } else {
        observation.assert_method("prepare", &[syn::parse_quote!("opaque SQL")]);
        observation.assert_method("get", &[syn::parse_quote!(3)]);
        observation.assert_method("id", &[syn::parse_quote!(__sql_fun_result.get(3)?)]);
        observation.assert_method("build", &[]);
    }
    assert!(!observation.has_macro("todo"));
}

#[rstest::rstest]
#[case::one(ConnectorSpec::render_query_one)]
#[case::optional(ConnectorSpec::render_query_opt)]
#[case::many(ConnectorSpec::render_query_many)]
#[case::execute(ConnectorSpec::render_execute)]
fn renders_check_mode_stubs(#[case] render: RenderFunction) {
    let spec = query_spec(syn::parse_quote!(
        fn generated(connection: Client) -> Result<Row, QueryError> {}
    ));
    assert_stub(render, &spec);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::one(ConnectorSpec::render_query_one)]
#[case::optional(ConnectorSpec::render_query_opt)]
#[case::many(ConnectorSpec::render_query_many)]
#[case::execute(ConnectorSpec::render_execute)]
fn renders_sqlite_check_mode_stubs(#[case] render: RenderFunction) {
    let mut spec = query_spec(syn::parse_quote!(
        fn generated(connection: Client) -> Result<Row, QueryError> {}
    ));
    spec.connector_spec = ConnectorSpec::realize(SqlDialect::SQLite, "rusqlite").unwrap();
    assert_stub(render, &spec);
}
