use super::QueryRequirement;

use crate::{
    QueryFunctionKind, QueryFunctionSpec, ResultType, connectors::ConnectorSpec,
    rust_syn::ItemFunction,
};
use ::proc_macro2::TokenStream;
use ::sql_fun_core::{CodegenOptions, PlaceholderedSql, ResolvedCteCatalog};

pub(super) fn specification() -> QueryFunctionSpec {
    let query = PlaceholderedSql::from_query("select ${id} from users");
    QueryFunctionSpec {
        function_kind: QueryFunctionKind::query_one(),
        connector_spec: ConnectorSpec::realize("postgresql".parse().unwrap(), "tokio-postgres")
            .unwrap(),
        resolved_cte_catalog: resolved_cte(&query),
        query,
        item_fn: ItemFunction::new(syn::parse_quote!(
            async fn lookup() {}
        )),
        connection: syn::parse_quote!(connection),
        result_type: ResultType {
            return_type: syn::parse_quote!(Row),
            error_type: syn::parse_quote!(QueryError),
        },
        interrupt_handler: None,
        lockfile_entry: None,
        datasource_name: "reporting".to_owned(),
        codegen_options: CodegenOptions::default(),
    }
}

fn resolved_cte(query: &PlaceholderedSql) -> ResolvedCteCatalog {
    let mut value = serde_json::to_value(ResolvedCteCatalog::from(query)).unwrap();
    value["cte_queries"]["active_users"] = serde_json::to_value(PlaceholderedSql::from_query(
        "select id from users where active = true",
    ))
    .unwrap();
    serde_json::from_value(value).unwrap()
}

pub(super) fn expanded_body() -> syn::Block {
    let spec = specification();
    let generated = QueryRequirement::emit_stub_function(
        &spec,
        &syn::parse_quote!(fn lookup()),
        &syn::parse_quote!("src/queries.rs"),
    )
    .unwrap();
    expand_line(&generated).block.as_ref().clone()
}

pub(super) fn expand_line(generated: &TokenStream) -> syn::ItemFn {
    // Model the expanded line! literal; panic expansion does not carry
    // protocol data and is deliberately irrelevant to the reader.
    syn::parse_str(&generated.to_string().replace("line ! ()", "42u32")).unwrap()
}

pub(super) fn source(body: &syn::Block) -> syn::File {
    syn::parse_quote!(fn lookup() #body)
}

pub(super) fn replace_marker(body: &mut syn::Block, name: &str, replacement: TokenStream) {
    let position = body.stmts.iter().position(|statement| {
        matches!(statement, syn::Stmt::Item(syn::Item::Const(item)) if item.ident == name)
    }).unwrap();
    body.stmts.remove(position);
    if !replacement.is_empty() {
        body.stmts
            .insert(position, syn::parse2(replacement).unwrap());
    }
}
