#![deny(missing_docs)]

//!
//! `sql-fun-codegen`: output / input for Rust AST
//!

mod common_options;
mod connectors;
mod cte_catalog;
mod error;
mod json_value_cache;
mod lockfile;
mod rust_syn;

use crate::{
    connectors::ConnectorSpec,
    cte_catalog::{CteCatalogSupport, CteReferenceCollection},
    rust_syn::{FragmentArm, SqlFragmentArgs},
};

use self::{
    common_options::SqlFunOptions,
    json_value_cache::JsonValueCache,
    lockfile::LockfileProvider,
    rust_syn::{FunctionArg, ItemFunction},
};

pub use self::error::SqlFunCodegenError;

use std::{collections::VecDeque, sync::Arc};

use proc_macro2::{Span, TokenStream};
use proc_macro2_diagnostic::DiagnosticResult;
use quote::{ToTokens, quote};
use sql_fun_core::{
    CteName, PlaceholderedSql, ResolvedCteCatalog, SqlFunArgs, SqlFunMetadata,
    lockfile::{Lockfile, LockfileEntry},
};
use syn::{Expr, FnArg, Ident, ItemFn, LitStr, Type, visit::Visit};

/// diagnostics
#[derive(Debug)]
pub enum SqlFunDiagnostic {
    /// CTE not defined in CTE catalog
    CteNotFound(CteName),
}

impl SqlFunDiagnostic {
    /// emit diagnostics
    ///
    /// # Errors
    ///
    /// Returns an error diagnostic when the CTE is not defined in the catalog.
    pub fn emit<T>(self) -> DiagnosticResult<T> {
        match self {
            Self::CteNotFound(m) => Self::emit_cte_not_found(&m),
        }
    }

    /// emit diagnostics
    ///
    /// # Errors
    ///
    /// Returns an error diagnostic when the CTE is not defined in the catalog.
    pub fn emit_cte_not_found<T>(name: &CteName) -> DiagnosticResult<T> {
        proc_macro2_diagnostic::error_spanned(
            Span::call_site(),
            format!("CTE {name} not found in CTE Catalog"),
        )
    }
}

/// diagnostics
#[derive(Debug)]
pub struct SqlFunDiagnosticCollection(Vec<SqlFunDiagnostic>);

/// attributes for `sql_fun::sql_query_one`
#[derive(Debug)]
pub struct SqlQueryOptions {
    common_options: SqlFunOptions,

    row_type: Option<Type>,

    error_type: Option<Type>,
}

impl SqlQueryOptions {
    fn parse_macro_attr(input: TokenStream) -> Result<Self, SqlFunCodegenError> {
        Ok(syn::parse2(input)?)
    }

    fn datasource(&self) -> Option<&str> {
        self.common_options.datasource()
    }

    const fn row_type(&self) -> Option<&Type> {
        self.row_type.as_ref()
    }

    const fn error_type(&self) -> Option<&Type> {
        self.error_type.as_ref()
    }
}

impl syn::parse::Parse for SqlQueryOptions {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let arguments = input.parse_terminated(syn::MetaNameValue::parse, syn::Token![,])?;
        let mut options = Self {
            common_options: SqlFunOptions::default(),
            row_type: None,
            error_type: None,
        };
        for argument in arguments {
            if options.common_options.set_name_value(&argument) {
                continue;
            }
            let ty = syn::parse2(quote::quote!(#argument.value))?;
            if argument.path.is_ident("row_type") {
                options.row_type = Some(ty);
            } else if argument.path.is_ident("error_type") {
                options.error_type = Some(ty);
            } else {
                return Err(syn::Error::new_spanned(argument.path, "unknown field"));
            }
        }
        Ok(options)
    }
}

/// default datasource name
pub const DEFAULT_DATASOURCE_NAME: &str = "default";

/// code generator function kind
#[derive(Clone, Copy, Debug)]
pub enum QueryFunctionKind {
    /// function for `sql_query_one`
    QueryOne,
    /// function for `sql_query_opt`
    QueryOpt,
    /// function for `sql_query_many`
    QueryMany,
    /// function for `sql_execute`
    Execute,
}

/// query function specifications
pub struct QueryFunctionSpec {
    function_kind: QueryFunctionKind,
    connector_spec: ConnectorSpec,
    query: PlaceholderedSql,
    item_fn: ItemFunction,
    resolved_cte_catalog: ResolvedCteCatalog,
    connection: Ident,
    result_type: ResultType,
    interrupt_handler: Option<Ident>,
    lockfile_entry: Option<LockfileEntry>,
    datasource_name: String,
}

impl QueryFunctionSpec {
    #[tracing::instrument(skip(self))]
    fn render_code(&self) -> Result<TokenStream, SqlFunCodegenError> {
        let query_function_name = self.query_function_name();
        tracing::info!("rendering {query_function_name}");
        match self.function_kind {
            QueryFunctionKind::QueryOne => self.connector_spec.render_query_one(self),
            QueryFunctionKind::QueryOpt => self.connector_spec.render_query_opt(self),
            QueryFunctionKind::QueryMany => self.connector_spec.render_query_many(self),
            QueryFunctionKind::Execute => self.connector_spec.render_execute(self),
        }
    }

    fn query_function_name(&self) -> String {
        self.item_fn.sig_ident()
    }

    const fn item_fn(&self) -> &ItemFunction {
        &self.item_fn
    }

    const fn connection_ident(&self) -> &Ident {
        &self.connection
    }

    const fn lockfile_entry(&self) -> Option<&LockfileEntry> {
        self.lockfile_entry.as_ref()
    }

    const fn row_type(&self) -> &Type {
        &self.result_type.return_type
    }

    const fn error_type(&self) -> &Type {
        &self.result_type.error_type
    }
}

/// conditional query match function
pub struct FragmentMatcher {
    function_kind: QueryFunctionKind,
    item_fn: ItemFunction,
    match_expr: Expr,
    arms: Vec<FragmentArm>,
}

impl FragmentMatcher {
    /// match expr
    #[must_use]
    pub const fn match_expr(&self) -> &Expr {
        &self.match_expr
    }

    fn match_arms(&self) -> &[FragmentArm] {
        &self.arms
    }

    /// render match
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    pub fn render_code(&self) -> Result<TokenStream, SqlFunCodegenError> {
        let is_async = self.item_fn.is_async();
        let inputs: Vec<_> = self.item_fn.argument_name_idents().cloned().collect();

        let expr = self.match_expr();
        let arms = self.match_arms();
        let arms = arms.iter().map(|arm| {
            let pat = arm.pat();
            let name = arm.arm_function_name(&self.item_fn);
            let mut introduce_args = arm.introduce_arg_idents().to_vec();
            let mut args = inputs.clone();
            args.append(&mut introduce_args);

            match (is_async, arm.guard()) {
                (true, Some(guard)) => quote! {
                    #pat if #guard => #name(#(#args),*).await
                },
                (false, Some(guard)) => quote! {
                    #pat if #guard => #name(#(#args),*)
                },
                (true, None) => quote! {
                    #pat => #name(#(#args),*).await
                },
                (false, None) => quote! {
                    #pat => #name(#(#args),*)
                },
            }
        });

        Ok(quote! {
            match #expr {
                #(#arms),*
            }
        }
        .to_token_stream())
    }

    /// generate match function
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    #[tracing::instrument(skip(self))]
    pub fn code_fn(&self) -> Result<TokenStream, SqlFunCodegenError> {
        let sig = self.item_fn.sig();
        let body = self.render_code()?;

        Ok(quote! {
            #sig {
                #body
            }
        })
    }
}

/// inner function for generate code
pub enum InnerFunction {
    /// querying function
    QueryFunction(Box<QueryFunctionSpec>),
    /// conditional branching function
    FragmentMatcher(Box<FragmentMatcher>),
}

impl From<QueryFunctionSpec> for InnerFunction {
    fn from(value: QueryFunctionSpec) -> Self {
        Self::QueryFunction(Box::new(value))
    }
}

impl From<FragmentMatcher> for InnerFunction {
    fn from(value: FragmentMatcher) -> Self {
        Self::FragmentMatcher(Box::new(value))
    }
}

impl InnerFunction {
    /// generate code
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    #[tracing::instrument(skip(self))]
    pub fn render_code(&self) -> Result<TokenStream, SqlFunCodegenError> {
        match self {
            Self::QueryFunction(q) => q.render_code(),
            Self::FragmentMatcher(f) => f.render_code(),
        }
    }
}

/// conditional query support
pub trait ConditionalQuery {
    /// get fragment spec from query
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    fn fragment_specs(
        item_fn: &ItemFunction,
        decoded_query: &PlaceholderedSql,
    ) -> Result<Vec<SqlFragmentArgs>, SqlFunCodegenError> {
        let mut fragment_specs = Vec::new();
        let mut fragment_idents = item_fn.get_fragment_idents();
        for fragment_name in decoded_query.fragment_names_by_usage_order() {
            let Some(fragment_attr) = item_fn.get_fragment_spec_attr(fragment_name)? else {
                return SqlFunCodegenError::fragment_spec_not_found(fragment_name);
            };
            fragment_idents.remove_first_match(|i| i == fragment_attr.fragment_name().as_str());
            fragment_specs.push(fragment_attr);
        }
        if !fragment_idents.is_empty() {
            return SqlFunCodegenError::fragment_spect_unused(fragment_idents);
        }
        Ok(fragment_specs)
    }

    /// expand conditional query
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    #[tracing::instrument(skip_all)]
    fn expand_conditional_query(
        metadata: &SqlFunMetadata,
        item: FragmentExpandQueueItem,
    ) -> Result<Vec<InnerFunction>, SqlFunCodegenError> {
        let mut expand_queue: VecDeque<FragmentExpandQueueItem> = VecDeque::new();
        expand_queue.push_back(item);
        let mut inner_functions = Vec::new();
        while let Some(queue_item) = expand_queue.pop_front() {
            let expanded = queue_item.process_item(metadata, &mut expand_queue)?;
            inner_functions.push(expanded);
        }
        Ok(inner_functions)
    }
}

/// function result type
#[derive(Debug, Clone)]
pub struct ResultType {
    return_type: Type,
    error_type: Type,
}

/// conditional query expansion item
#[derive(bon::Builder)]
pub struct FragmentExpandQueueItem {
    function_kind: QueryFunctionKind,
    query: PlaceholderedSql,
    fragment_specs: VecDeque<SqlFragmentArgs>,
    item_fn: ItemFunction,
    cte_references: CteReferenceCollection,
    connector_spec: ConnectorSpec,
    connection: Ident,
    result_type: ResultType,
    lockfile: Option<Arc<Lockfile>>,
    datasource_name: String,
}

impl FragmentExpandQueueItem {
    /// expand item
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    #[tracing::instrument(skip(self, metadata, queue))]
    pub fn process_item(
        self,
        metadata: &SqlFunMetadata,
        queue: &mut VecDeque<Self>,
    ) -> Result<InnerFunction, SqlFunCodegenError> {
        let function_kind = self.function_kind;
        let mut fragment_specs = self.fragment_specs;
        let item_fn = self.item_fn;
        let cte_references = self.cte_references;
        let query = self.query;
        let connector_spec = self.connector_spec;
        let result_type = self.result_type;
        let connection = self.connection;
        let lockfile = self.lockfile;
        let datasource_name = self.datasource_name;

        if let Some(fragment) = fragment_specs.pop_front() {
            tracing::info!("processing {fragment:?}");
            let mut arms = Vec::new();
            let mut child_fn = item_fn.clone();

            for arg in fragment.consume_args() {
                child_fn.remove_arg_by_name(arg);
            }
            for match_arm in fragment.match_arms() {
                arms.push(match_arm.clone());

                let mut child_fn = child_fn.clone();
                child_fn.set_sig_ident(match_arm.arm_function_name(&child_fn));

                for arg in match_arm.introduce_args() {
                    child_fn.append_arg(&FnArg::Typed(arg.clone()));
                }

                let embedded_query =
                    query.embed_fragment(fragment.fragment_name(), match_arm.sql_fragment())?;

                queue.push_back(Self {
                    function_kind,
                    query: embedded_query,
                    fragment_specs: fragment_specs.clone(),
                    item_fn: child_fn,
                    cte_references: cte_references.clone(),
                    connector_spec: connector_spec.clone(),
                    connection: connection.clone(),
                    result_type: result_type.clone(),
                    lockfile: lockfile.clone(),
                    datasource_name: datasource_name.clone(),
                });
            }
            let match_expr = fragment.match_expr();
            let matcher = FragmentMatcher {
                function_kind,
                item_fn,
                arms,
                match_expr: match_expr.clone(),
            };
            Ok(InnerFunction::from(matcher))
        } else {
            let interrupt_handler = item_fn
                .find_attributed_argument_ident(&connector_spec.interrupt_arg_attr())?
                .cloned();
            let resolved_cte_catalog = cte_references.resolve(metadata, &query)?;
            let query_finger_print = &resolved_cte_catalog.finger_print()?;

            let lockfile_entry = if let Some(lockfile) = lockfile {
                let Some(lockfile_entry) = lockfile.get_entry(query_finger_print)? else {
                    SqlFunCodegenError::missing_lockfile_entry()?;
                    SqlFunCodegenError::never()
                };
                Some(lockfile_entry)
            } else {
                None
            };

            Ok(InnerFunction::from(QueryFunctionSpec {
                function_kind,
                query,
                item_fn,
                resolved_cte_catalog,
                connection,
                result_type,
                interrupt_handler,
                lockfile_entry,
                connector_spec: connector_spec.clone(),
                datasource_name,
            }))
        }
    }
}

/// `sql_fun::sql_query_one` proc macro item
pub struct QueryFunctionForGenerate {
    item_fn: ItemFunction,
    inner_functions: Vec<InnerFunction>,
}

trait VecExt<T> {
    fn remove_first_match<F>(&mut self, predicate: F) -> Option<T>
    where
        F: FnMut(&T) -> bool;
}

impl<T> VecExt<T> for Vec<T> {
    fn remove_first_match<F>(&mut self, predicate: F) -> Option<T>
    where
        F: FnMut(&T) -> bool,
    {
        let index = self.iter().position(predicate)?;
        Some(self.remove(index))
    }
}

impl ConditionalQuery for QueryFunctionForGenerate {}
impl CteCatalogSupport for QueryFunctionForGenerate {}

/// code generator macro implementation
pub trait QueryFunctionBase: CteCatalogSupport + ConditionalQuery
where
    Self: Sized,
{
    /// create instance
    fn new(item_fn: ItemFunction, inner_functions: Vec<InnerFunction>) -> Self;

    /// parse macro input
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    #[tracing::instrument(err, skip(args, attrs, item_fn))]
    fn try_from_macro_input(
        args: &SqlFunArgs,
        attrs: TokenStream,
        item_fn: ItemFn,
        kind: QueryFunctionKind,
    ) -> Result<Self, SqlFunCodegenError> {
        let item_fn = ItemFunction::new(item_fn);
        let query_function_name = item_fn.sig_ident();
        tracing::info!("query function name : {query_function_name}");

        let options = SqlQueryOptions::parse_macro_attr(attrs)?;

        let datasource_name = String::from(options.datasource().unwrap_or(DEFAULT_DATASOURCE_NAME));
        tracing::info!("datasource selected : {datasource_name}");

        let metadata_collection = args.load_metadata()?;
        let Some(metadata) = metadata_collection.get(&datasource_name) else {
            return SqlFunCodegenError::datasource_not_found(&datasource_name);
        };
        let connector_spec = Self::connector_spec(metadata, &datasource_name)?;
        item_fn.validate_async(connector_spec.require_async())?;
        let decoded_query = options.common_options.query()?;
        let fragment_specs = Self::fragment_specs(&item_fn, &decoded_query)?;
        let cte_references = Self::cte_references(&item_fn, metadata)?;
        let connection = Self::connection_parameter_ident(&item_fn)?;
        let result_type = Self::result_type(&options, &item_fn)?;

        let item = if args.is_check_mode() {
            FragmentExpandQueueItem::builder()
                .function_kind(kind)
                .query(decoded_query)
                .fragment_specs(fragment_specs.into())
                .item_fn(item_fn.clone())
                .cte_references(cte_references)
                .connector_spec(connector_spec.clone())
                .connection(connection)
                .result_type(result_type)
                .datasource_name(datasource_name)
                .build()
        } else {
            let lockfile = LockfileProvider::load_datasource(args, &datasource_name)?;
            FragmentExpandQueueItem::builder()
                .lockfile(lockfile)
                .function_kind(kind)
                .query(decoded_query)
                .fragment_specs(fragment_specs.into())
                .item_fn(item_fn.clone())
                .cte_references(cte_references)
                .connector_spec(connector_spec.clone())
                .connection(connection)
                .result_type(result_type)
                .datasource_name(datasource_name)
                .build()
        };

        let inner_functions = Self::expand_conditional_query(metadata, item)?;

        Ok(Self::new(item_fn, inner_functions))
    }

    /// get connector spec from metadata
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    #[tracing::instrument(err, skip(metadata))]
    fn connector_spec(
        metadata: &SqlFunMetadata,
        datasource_name: &str,
    ) -> Result<ConnectorSpec, SqlFunCodegenError> {
        let dialect = metadata.sql_dialect();
        let Some(connector) = metadata.connector() else {
            return SqlFunCodegenError::connector_not_specified(datasource_name);
        };

        let connector_spec = ConnectorSpec::realize(dialect, connector)?;
        Ok(connector_spec)
    }

    /// get connection parameter ident
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    #[tracing::instrument(err, skip(item_fn))]
    fn connection_parameter_ident(item_fn: &ItemFunction) -> Result<Ident, SqlFunCodegenError> {
        let Some(connection_arg) = item_fn.argument(0) else {
            return SqlFunCodegenError::require_connection_arg();
        };
        let connection_arg = FunctionArg::new(connection_arg);
        let connection = connection_arg.typed_ident()?.clone();
        Ok(connection)
    }

    /// get result type
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    #[tracing::instrument(ret, err, skip(options, item_fn))]
    fn result_type(
        options: &SqlQueryOptions,
        item_fn: &ItemFunction,
    ) -> Result<ResultType, SqlFunCodegenError> {
        let return_type = Self::row_type(options, item_fn)?;
        let error_type = Self::error_type(options, item_fn)?;
        let result_type = ResultType {
            return_type,
            error_type,
        };
        Ok(result_type)
    }

    /// get row type
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    #[tracing::instrument(err, skip(options, item_fn))]
    fn row_type(
        options: &SqlQueryOptions,
        item_fn: &ItemFunction,
    ) -> Result<Type, SqlFunCodegenError> {
        let return_type = if let Some(row_type) = options.row_type() {
            row_type.clone()
        } else {
            let Some(return_type) = item_fn.result_ok_type() else {
                return SqlFunCodegenError::require_row_type();
            };
            return_type
        };
        Ok(return_type)
    }

    /// get error type
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    #[tracing::instrument(err, skip(options, item_fn))]
    fn error_type(
        options: &SqlQueryOptions,
        item_fn: &ItemFunction,
    ) -> Result<Type, SqlFunCodegenError> {
        let error_type = if let Some(error_type) = options.error_type() {
            error_type.clone()
        } else {
            let Some(error_type) = item_fn.result_err_type() else {
                return SqlFunCodegenError::require_error_type();
            };
            error_type
        };
        Ok(error_type)
    }
}

impl QueryFunctionBase for QueryFunctionForGenerate {
    fn new(item_fn: ItemFunction, inner_functions: Vec<InnerFunction>) -> Self {
        Self {
            item_fn,
            inner_functions,
        }
    }
}

impl QueryFunctionForGenerate {
    /// get generated code
    ///
    /// # Errors
    ///
    /// `SqlFunCodegenError`
    #[tracing::instrument(err, skip(self))]
    pub fn code(self) -> Result<TokenStream, SqlFunCodegenError> {
        let mut item_fn = self.item_fn.clone();
        let inner_fn_name = Ident::new(&item_fn.sig_ident(), Span::call_site());
        let fn_args: Vec<_> = item_fn.argument_name_idents().cloned().collect();
        let is_async = item_fn.is_async();
        let block = item_fn.block_mut();
        for inner_fn in self.inner_functions {
            let inner_fn_code = inner_fn.render_code()?;
            tracing::info!("pushing inner function to fn body");
            block.stmts.push(syn::parse2(inner_fn_code)?);
        }
        tracing::info!("inner functions generated");

        let call_inner_fn = if is_async {
            quote::quote! {
                return #inner_fn_name( #(#fn_args),* ).await;
            }
        } else {
            quote::quote! {
                return #inner_fn_name( #(#fn_args),* );
            }
        };
        block.stmts.push(syn::parse2(call_inner_fn)?);
        let item_fn = item_fn.into_inner();
        let result = quote::quote! { #item_fn };

        tracing::info!("final generated source \n{}", result.to_string());

        Ok(result)
    }
}

/// Source embeddable SQL Statement string
#[derive(Debug)]
pub struct SQLStatement {
    span: Span,
    sql_text: String,
    datasource: String,
}

impl SQLStatement {
    /// create instance
    #[must_use]
    pub fn new(sql_text: &str, span: &Span, datasource: &str) -> Self {
        Self {
            span: *span,
            sql_text: String::from(sql_text),
            datasource: String::from(datasource),
        }
    }

    /// convert to [`TokenStream`]
    #[must_use]
    pub fn into_quote(self) -> TokenStream {
        let sql_text = LitStr::new(&self.sql_text, self.span);
        let source_path = LitStr::new(&self.span.file(), self.span);
        let datasource = LitStr::new(&self.datasource, self.span);

        quote::quote! {
            #[expect(dead_code)]
            fn __sql_fun_query_info() {
                const SQL_TEXT: &str = #sql_text;
                const SOURCE: &str = #source_path;
                const DATASOURCE: &str = #datasource;
            }
        }
    }

    const QUERY_INFO_FUN_NAME: &str = "__sql_fun_query_info";
}

/// Source embedded SQL Statement
#[derive(Debug)]
pub struct EncodedSQLStatement {
    sql_text: LitStr,
    file: LitStr,
    datasource: LitStr,
}

impl EncodedSQLStatement {
    /// SQL text literal
    #[must_use]
    pub const fn sql_text(&self) -> &LitStr {
        &self.sql_text
    }

    /// embedded location file
    #[must_use]
    pub const fn file(&self) -> &LitStr {
        &self.file
    }

    /// specified datasource
    #[must_use]
    pub const fn datasource(&self) -> &LitStr {
        &self.datasource
    }
}

/// collect `EncodedSQLStatement` from `syn::File`
#[derive(Debug, Default)]
pub struct SQLQueryInfoVisitor {
    last_lit_str: Option<LitStr>,
    sql_text: Option<LitStr>,
    file: Option<LitStr>,
    datasource: Option<LitStr>,
    results: Vec<EncodedSQLStatement>,
}

impl SQLQueryInfoVisitor {
    /// get SQL statements embedded in source code
    #[must_use]
    pub fn sql_statements(file: &syn::File) -> Vec<EncodedSQLStatement> {
        let mut visitor = Self::default();
        visitor.visit_file(file);
        visitor.results
    }
}

impl<'ast> Visit<'ast> for SQLQueryInfoVisitor {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if i.sig.ident == SQLStatement::QUERY_INFO_FUN_NAME {
            self.visit_block(&i.block);
            if let Some(sql_text) = self.sql_text.clone()
                && let Some(file) = self.file.clone()
                && let Some(datasource) = self.datasource.clone()
            {
                self.sql_text = None;
                self.file = None;
                self.results.push(EncodedSQLStatement {
                    sql_text,
                    file,
                    datasource,
                });
            }
        } else {
            syn::visit::visit_item_fn(self, i);
        }
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        if i.ident == "SQL_TEXT" {
            self.visit_expr(&i.expr);
            self.sql_text = self.last_lit_str.clone();
        }

        if i.ident == "SOURCE" {
            self.visit_expr(&i.expr);
            self.file = self.last_lit_str.clone();
        }

        if i.ident == "DATASOUCE" {
            self.visit_expr(&i.expr);
            self.datasource = self.last_lit_str.clone();
        }
    }

    fn visit_lit_str(&mut self, i: &'ast syn::LitStr) {
        self.last_lit_str = Some(i.clone());
    }
}
