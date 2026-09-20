use std::{path::Path, str::FromStr};

use proc_macro2::{Span, TokenStream};
use sql_fun_core::{SqlDialect, lockfile::LockfileEntry};
use syn::{Ident, LitInt, LitStr};

use crate::{QueryFunctionSpec, error::SqlFunCodegenError};

pub trait ConnectorCodeGenerator: std::fmt::Debug {
    fn render_query_one(&self, spec: &QueryFunctionSpec)
    -> Result<TokenStream, SqlFunCodegenError>;

    fn render_query_opt(&self, spec: &QueryFunctionSpec)
    -> Result<TokenStream, SqlFunCodegenError>;

    fn render_query_many(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError>;

    fn render_execute(&self, spec: &QueryFunctionSpec) -> Result<TokenStream, SqlFunCodegenError>;
}

/// database connector library spec
#[derive(Debug, Clone)]
pub struct ConnectorSpec {
    kind: ConnectorKind,
}

impl ConnectorSpec {
    /// realize connector with dialect
    ///
    /// # Errors
    ///
    /// [`SqlFunCodegenError`]
    ///
    pub fn realize(dialect: SqlDialect, connector_name: &str) -> Result<Self, SqlFunCodegenError> {
        let kind = ConnectorKind::from_str(connector_name)?;
        if !kind.compatible_with(dialect) {
            return SqlFunCodegenError::dialect_incompatible_with_connector(
                dialect,
                connector_name,
            )?;
        }
        Ok(Self { kind })
    }

    /// get async mode spec
    #[must_use]
    pub const fn require_async(&self) -> Option<bool> {
        self.kind.require_async()
    }

    /// interrupt argument attribute ident
    #[must_use]
    pub fn interrupt_arg_attr(&self) -> Ident {
        Ident::new(self.kind.interrupt_arg_attr_name(), Span::call_site())
    }

    /// render query one code
    #[tracing::instrument(err, skip(spec))]
    pub fn render_query_one(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let code_generator = self.kind.code_generator();
        code_generator.render_query_one(spec)
    }

    /// render query opt code
    #[tracing::instrument(err, skip(spec))]
    pub fn render_query_opt(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let code_generator = self.kind.code_generator();
        code_generator.render_query_opt(spec)
    }

    /// render query many code
    #[tracing::instrument(err, skip(spec))]
    pub fn render_query_many(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let code_generator = self.kind.code_generator();
        code_generator.render_query_many(spec)
    }

    /// render execute query code
    #[tracing::instrument(err, skip(spec))]
    pub fn render_execute(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let code_generator = self.kind.code_generator();
        code_generator.render_execute(spec)
    }
}

/// Connector kind
#[derive(Clone, Copy, Debug)]
pub enum ConnectorKind {
    /// use `tokio-postgres` connector
    TokioPostgres,
}

impl ConnectorKind {
    /// return true when connector works on dialect
    #[must_use]
    pub const fn compatible_with(self, dialect: SqlDialect) -> bool {
        match (self, dialect) {
            (Self::TokioPostgres, SqlDialect::PostgreSQL) => true,
            #[allow(unreachable_patterns)]
            // TODO: change to expect, rust 1.97.1 unfulfilled warning
            _ => false,
        }
    }

    /// return true when async required
    ///
    /// # Return values
    ///
    /// - Some(true) : must async
    /// - Some(false) : must not async
    /// - None : async or not async
    ///
    #[must_use]
    #[expect(clippy::unnecessary_wraps)]
    pub const fn require_async(self) -> Option<bool> {
        match self {
            Self::TokioPostgres => Some(true),
        }
    }

    /// interrupt attribute ident
    #[must_use]
    pub const fn interrupt_arg_attr_name(self) -> &'static str {
        match self {
            Self::TokioPostgres => "interrupt_future",
        }
    }

    pub fn code_generator(self) -> Box<dyn ConnectorCodeGenerator> {
        match self {
            Self::TokioPostgres => Box::new(TokioPostgresCodeGenerator::new()),
        }
    }
}

impl FromStr for ConnectorKind {
    type Err = SqlFunCodegenError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tokio-postgres" => Ok(Self::TokioPostgres),
            _ => {
                SqlFunCodegenError::connector_not_supported(s)?;
                SqlFunCodegenError::never()
            }
        }
    }
}

#[derive(Clone, Debug)]
struct TokioPostgresCodeGenerator {}

impl TokioPostgresCodeGenerator {
    pub const fn new() -> Self {
        Self {}
    }

    fn source_file() -> LitStr {
        let call_site = Span::call_site();
        let source_file = call_site.local_file().and_then(|path| {
            let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")?;
            path.strip_prefix(manifest_dir).ok().map(Path::to_path_buf)
        });

        let source_file = source_file
            .as_deref()
            .and_then(Path::to_str)
            .unwrap_or("<unknown>");

        LitStr::new(source_file, proc_macro2::Span::call_site())
    }

    fn parameters(lockfile_entry: &LockfileEntry) -> Vec<TokenStream> {
        let parameters: Vec<_> = lockfile_entry
            .bind_parameters()
            .iter()
            .map(|p| {
                let name = Ident::new(p.name(), Span::call_site());
                quote::quote! { &#name }
            })
            .collect();
        parameters
    }

    fn builder_setter_calls(lockfile_entry: &LockfileEntry) -> Vec<TokenStream> {
        let builder_setter_calls: Vec<_> = lockfile_entry
            .result_set_shape()
            .iter()
            .map(|field| {
                let field_name = Ident::new(field.field_name(), Span::call_site());
                let field_index = LitInt::new(&field.field_index().to_string(), Span::call_site());
                quote::quote! {
                    builder.#field_name( result.try_get(#field_index)? );
                }
            })
            .collect();
        builder_setter_calls
    }

    fn option_row_type(spec: &QueryFunctionSpec) -> &syn::Type {
        let row_type = spec.row_type();
        let syn::Type::Path(type_path) = row_type else {
            return row_type;
        };
        let Some(segment) = type_path.path.segments.last() else {
            return row_type;
        };
        if segment.ident != "Option" {
            return row_type;
        }
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return row_type;
        };
        let Some(syn::GenericArgument::Type(inner_type)) = arguments.args.first() else {
            return row_type;
        };
        inner_type
    }

    fn row_type_from_fn_bounds(
        bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
    ) -> Option<syn::Type> {
        bounds.iter().find_map(|bound| {
            let syn::TypeParamBound::Trait(bound) = bound else {
                return None;
            };
            let segment = bound.path.segments.last()?;
            if segment.ident != "Fn" {
                return None;
            }
            let syn::PathArguments::Parenthesized(arguments) = &segment.arguments else {
                return None;
            };
            arguments
                .inputs
                .iter()
                .nth(1)
                .map(|argument| argument.ty.clone())
        })
    }

    fn query_many_arguments(
        spec: &QueryFunctionSpec,
    ) -> Result<(Ident, Ident, syn::Type), SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let mut inputs = item_fn.sig().inputs.iter().rev();
        let Some(syn::FnArg::Typed(handler_arg)) = inputs.next() else {
            return Err(syn::Error::new_spanned(
                item_fn.sig(),
                "sql_query_many requires handler as its last parameter",
            )
            .into());
        };
        let Some(syn::FnArg::Typed(collector_arg)) = inputs.next() else {
            return Err(syn::Error::new_spanned(
                item_fn.sig(),
                "sql_query_many requires collector before its handler",
            )
            .into());
        };
        let syn::Pat::Ident(handler_pat) = &*handler_arg.pat else {
            return SqlFunCodegenError::require_identifier();
        };
        let syn::Pat::Ident(collector_pat) = &*collector_arg.pat else {
            return SqlFunCodegenError::require_identifier();
        };
        let syn::Type::Path(handler_type) = &*handler_arg.ty else {
            return Err(syn::Error::new_spanned(
                &handler_arg.ty,
                "sql_query_many handler must be a generic type parameter",
            )
            .into());
        };
        let Some(handler_type) = handler_type.path.get_ident() else {
            return Err(syn::Error::new_spanned(
                &handler_arg.ty,
                "sql_query_many handler must be a generic type parameter",
            )
            .into());
        };
        let Some(handler_generic) = item_fn.sig().generics.params.iter().find_map(|parameter| {
            let syn::GenericParam::Type(parameter) = parameter else {
                return None;
            };
            (parameter.ident == *handler_type).then_some(parameter)
        }) else {
            return Err(syn::Error::new_spanned(
                &handler_arg.ty,
                "sql_query_many handler must be a generic type parameter",
            )
            .into());
        };
        let row_type = Self::row_type_from_fn_bounds(&handler_generic.bounds).or_else(|| {
            item_fn
                .sig()
                .generics
                .where_clause
                .as_ref()
                .and_then(|where_clause| {
                    where_clause.predicates.iter().find_map(|predicate| {
                        let syn::WherePredicate::Type(predicate) = predicate else {
                            return None;
                        };
                        let syn::Type::Path(bounded_type) = &predicate.bounded_ty else {
                            return None;
                        };
                        (bounded_type.path.get_ident() == Some(handler_type))
                            .then(|| Self::row_type_from_fn_bounds(&predicate.bounds))
                            .flatten()
                    })
                })
        });
        let Some(row_type) = row_type else {
            return Err(syn::Error::new_spanned(
                handler_generic,
                "sql_query_many handler must implement Fn(Collector, Row) -> Future",
            )
            .into());
        };

        Ok((
            collector_pat.ident.clone(),
            handler_pat.ident.clone(),
            row_type,
        ))
    }

    fn query_one_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> TokenStream {
        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = Self::parameters(lockfile_entry);
        let row_type = spec.row_type();
        let builder_setter_calls = Self::builder_setter_calls(lockfile_entry);
        let source_file = Self::source_file();
        if let Some(interrupt) = &spec.interrupt_handler {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let params = &[#(#parameters),*];
                    let result = tokio::select! {
                        res = #connection.query_one( #query, params ) => res,
                        interrupt_result = #interrupt => Err(interrupt_result)
                    }
                    let result = result?;
                    let mut builder = #row_type::builder();

                    #(#builder_setter_calls)*

                    Ok( builder.build()? )
                }
            }
        } else {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let params = &[#(#parameters),*];
                    let result = #connection.query_one( #query, params ).await?;
                    let mut builder = #row_type::builder();

                    #(#builder_setter_calls)*

                    Ok( builder.build()? )
                }
            }
        }
    }

    fn query_opt_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> TokenStream {
        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = Self::parameters(lockfile_entry);
        let row_type = Self::option_row_type(spec);
        let builder_setter_calls = Self::builder_setter_calls(lockfile_entry);
        let source_file = Self::source_file();
        if let Some(interrupt) = &spec.interrupt_handler {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let params = &[#(#parameters),*];
                    let result = tokio::select! {
                        res = #connection.query_opt( #query, params ) => res,
                        interrupt_result = #interrupt => Err(interrupt_result)
                    };
                    let Some(result) = result? else {
                        return Ok(None);
                    };
                    let mut builder = #row_type::builder();

                    #(#builder_setter_calls)*

                    Ok(Some(builder.build()?))
                }
            }
        } else {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let params = &[#(#parameters),*];
                    let result = #connection.query_opt( #query, params ).await?;
                    let Some(result) = result else {
                        return Ok(None);
                    };
                    let mut builder = #row_type::builder();

                    #(#builder_setter_calls)*

                    Ok(Some(builder.build()?))
                }
            }
        }
    }

    fn query_many_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = Self::parameters(lockfile_entry);
        let (collector, handler, row_type) = Self::query_many_arguments(spec)?;
        let builder_setter_calls = Self::builder_setter_calls(lockfile_entry);
        let source_file = Self::source_file();
        let function = if let Some(interrupt) = &spec.interrupt_handler {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    use futures_util::TryStreamExt as _;

                    let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let rows = tokio::select! {
                        res = #connection.query_raw( #query, params.iter().copied() ) => res,
                        interrupt_result = #interrupt => return Err(interrupt_result)
                    }?;
                    let mut rows = std::pin::pin!(rows);
                    while let Some(result) = tokio::select! {
                        res = rows.try_next() => res?,
                        interrupt_result = #interrupt => return Err(interrupt_result)
                    } {
                        let mut builder = #row_type::builder();

                        #(#builder_setter_calls)*

                        #collector = #handler(#collector, builder.build()?).await?;
                    }

                    Ok(#collector)
                }
            }
        } else {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    use futures_util::TryStreamExt as _;

                    let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let rows = #connection.query_raw( #query, params.iter().copied() ).await?;
                    let mut rows = std::pin::pin!(rows);
                    while let Some(result) = rows.try_next().await? {
                        let mut builder = #row_type::builder();

                        #(#builder_setter_calls)*

                        #collector = #handler(#collector, builder.build()?).await?;
                    }

                    Ok(#collector)
                }
            }
        };
        Ok(function)
    }

    fn execute_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> TokenStream {
        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = Self::parameters(lockfile_entry);
        let source_file = Self::source_file();
        if let Some(interrupt) = &spec.interrupt_handler {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let result = tokio::select! {
                        res = #connection.execute( #query, params ) => res,
                        interrupt_result = #interrupt => Err(interrupt_result)
                    }?;

                    Ok(result)
                }
            }
        } else {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let result = #connection.execute( #query, params ).await?;

                    Ok(result)
                }
            }
        }
    }

    fn stub_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let source_file = Self::source_file();
        let datasource_name = LitStr::new(&spec.datasource_name, Span::call_site());
        let query = LitStr::new(&spec.query.encode(), Span::call_site());
        let cte_resolved = serde_json::to_string(&spec.resolved_cte_catalog)?;
        let cte_resolved = LitStr::new(&cte_resolved, Span::call_site());
        let function = quote::quote! {
            #sig
            {
                #[expect(dead_code)]
                const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                #[expect(dead_code)]
                const __SQL_FUN_SOURCE_LINE: u32 = line!();
                #[expect(dead_code)]
                const __SQL_FUN_DATASOURCE: &str = #datasource_name;
                #[expect(dead_code)]
                const __SQL_FUN_QUERY: &str = #query;
                #[expect(dead_code)]
                const __SQL_FUN_USING_CTE: &str = #cte_resolved;

                todo!("please re run `sql-fun check`");
            }
        };
        Ok(function)
    }
}

impl ConnectorCodeGenerator for TokioPostgresCodeGenerator {
    fn render_query_one(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = item_fn.sig();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_one_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = Self::stub_function(spec, sig)?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_query_opt(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = item_fn.sig();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_opt_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = Self::stub_function(spec, sig)?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_query_many(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = item_fn.sig();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_many_function(spec, sig, connection, lockfile_entry)?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = Self::stub_function(spec, sig)?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_execute(&self, spec: &QueryFunctionSpec) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = item_fn.sig();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::execute_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = Self::stub_function(spec, sig)?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }
}
