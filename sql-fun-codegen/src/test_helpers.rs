//! Lightweight, parameterized fixtures for connector tests.

use crate::{
    QueryFunctionKind, QueryFunctionSpec, ResultType, connectors::ConnectorSpec,
    rust_syn::ItemFunction,
};
use ::sql_fun_core::{
    CodegenOptions, PlaceholderedSql, ResolvedCteCatalog, SqlDialect, lockfile::LockfileEntry,
};
use ::syn::visit::Visit;

pub(crate) fn query_spec(function: syn::ItemFn) -> QueryFunctionSpec {
    let item_fn = ItemFunction::new(function);
    let query = PlaceholderedSql::from_query("select ${id} from users");
    QueryFunctionSpec {
        parameter_bindings: item_fn.parameter_bindings().unwrap(),
        function_kind: QueryFunctionKind::query_one(),
        connector_spec: ConnectorSpec::realize(SqlDialect::PostgreSQL, "tokio-postgres").unwrap(),
        resolved_cte_catalog: ResolvedCteCatalog::from(&query),
        query,
        item_fn,
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

pub(crate) fn lockfile_entry(
    sql: &str,
    bindings: &[(&str, u16)],
    fields: &[(&str, u16)],
) -> LockfileEntry {
    // Use the serialized contract rather than changing the core model for tests.
    serde_json::from_value(serde_json::json!({
        "query_finger_print": "fixture",
        "query_name": "generated",
        "execute_sql": sql,
        "bind_parameters": bindings.iter().map(|(name, index)| serde_json::json!({"name": name, "parameter_index": index})).collect::<Vec<_>>(),
        "result_set_shape": fields.iter().map(|(name, index)| serde_json::json!({"field_name": name, "field_index": index, "sql_type": "int8"})).collect::<Vec<_>>()
    })).unwrap()
}

/// AST observations used to assert contracts without token-format snapshots.
#[derive(Default)]
pub(crate) struct GeneratedFunction {
    methods: Vec<syn::ExprMethodCall>,
    arrays: Vec<syn::ExprArray>,
    macros: Vec<syn::Macro>,
    constants: Vec<syn::ItemConst>,
}

impl GeneratedFunction {
    pub(crate) fn inspect(function: &syn::ItemFn) -> Self {
        let mut observation = Self::default();
        observation.visit_item_fn(function);
        observation
    }
}

impl GeneratedFunction {
    pub(crate) fn assert_method(&self, name: &str, arguments: &[syn::Expr]) {
        assert!(
            self.methods
                .iter()
                .any(|call| call.method == name && call.args.iter().eq(arguments)),
            "missing method {name}({arguments:?})"
        );
    }
}

impl GeneratedFunction {
    pub(crate) fn assert_binding_array(&self, expressions: &[syn::Expr]) {
        assert!(
            self.arrays
                .iter()
                .any(|array| array.elems.iter().eq(expressions)),
            "missing binding array {expressions:?}"
        );
    }
}

impl GeneratedFunction {
    pub(crate) fn has_macro(&self, name: &str) -> bool {
        self.macros.iter().any(|item| {
            item.path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == name)
        })
    }
}

impl GeneratedFunction {
    pub(crate) fn string_constant(&self, name: &str) -> String {
        let constant = self
            .constants
            .iter()
            .find(|item| item.ident == name)
            .expect("generated constant");
        let syn::Expr::Lit(value) = &*constant.expr else {
            panic!("expected literal constant")
        };
        let syn::Lit::Str(value) = &value.lit else {
            panic!("expected string constant")
        };
        value.value()
    }
}

impl<'ast> Visit<'ast> for GeneratedFunction {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.methods.push(node.clone());
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_array(&mut self, node: &'ast syn::ExprArray) {
        self.arrays.push(node.clone());
        syn::visit::visit_expr_array(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.macros.push(node.clone());
        syn::visit::visit_macro(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.constants.push(node.clone());
        syn::visit::visit_item_const(self, node);
    }
}

/// One of the four connector rendering entry points.
pub(crate) type RenderFunction =
    fn(
        &ConnectorSpec,
        &QueryFunctionSpec,
    ) -> Result<::proc_macro2::TokenStream, crate::SqlFunCodegenError>;

pub(crate) fn assert_stub(render: RenderFunction, spec: &QueryFunctionSpec) {
    let function: syn::ItemFn = syn::parse2(render(&spec.connector_spec, spec).unwrap()).unwrap();
    assert_eq!(function.sig, spec.item_fn.codegen_signature());
    let observation = GeneratedFunction::inspect(&function);
    assert!(observation.has_macro("todo"));
    assert_eq!(
        observation.string_constant("__SQL_FUN_DATASOURCE"),
        spec.datasource_name
    );
    assert_eq!(
        observation.string_constant("__SQL_FUN_QUERY"),
        spec.query.encode()
    );
    assert_eq!(
        observation.string_constant("__SQL_FUN_CODEGEN_VERSION"),
        env!("CARGO_PKG_VERSION")
    );
    let actual: ::sql_fun_core::ResolvedCteCatalog =
        serde_json::from_str(&observation.string_constant("__SQL_FUN_USING_CTE")).unwrap();
    assert_eq!(
        actual.finger_print().unwrap(),
        spec.resolved_cte_catalog.finger_print().unwrap()
    );
}
