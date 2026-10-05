use std::{path::Path, str::FromStr};

use proc_macro2::{Span, TokenStream};
use sql_fun_core::{SqlDialect, lockfile::LockfileEntry};
use syn::{Ident, LitInt, LitStr};

use crate::rust_syn::GeneratedIdentifiers;
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

    /// use `rusqlite`
    #[cfg(feature = "sqlite")]
    Rusqlite,
}

impl ConnectorKind {
    /// return true when connector works on dialect
    #[must_use]
    pub const fn compatible_with(self, dialect: SqlDialect) -> bool {
        match (self, dialect) {
            (Self::TokioPostgres, SqlDialect::PostgreSQL) => true,
            #[cfg(feature = "sqlite")]
            (Self::Rusqlite, SqlDialect::SQLite) => true,
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
            #[cfg(feature = "sqlite")]
            Self::Rusqlite => Some(false),
        }
    }

    /// interrupt attribute ident
    #[must_use]
    pub const fn interrupt_arg_attr_name(self) -> &'static str {
        match self {
            Self::TokioPostgres => "interrupt_future",
            #[cfg(feature = "sqlite")]
            Self::Rusqlite => "interrupt_channel",
        }
    }

    pub fn code_generator(self) -> Box<dyn ConnectorCodeGenerator> {
        match self {
            Self::TokioPostgres => Box::new(TokioPostgresCodeGenerator::new()),
            #[cfg(feature = "sqlite")]
            Self::Rusqlite => Box::new(RusqliteCodeGenerator::new()),
        }
    }
}

impl FromStr for ConnectorKind {
    type Err = SqlFunCodegenError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tokio-postgres" => Ok(Self::TokioPostgres),
            #[cfg(feature = "sqlite")]
            "rusqlite" => Ok(Self::Rusqlite),
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

    fn builder_setter_calls(
        lockfile_entry: &LockfileEntry,
        builder: &crate::row_builder::RowBuilder<'_>,
        result: &Ident,
    ) -> Vec<TokenStream> {
        let mut calls = Vec::new();
        for field in lockfile_entry.result_set_shape() {
            let name = Ident::new(field.field_name(), Span::call_site());
            let index = LitInt::new(&field.field_index().to_string(), Span::call_site());
            calls.push(builder.setter(&name, &quote::quote! { #result.try_get(#index)? }));
        }
        calls
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
            if parameter.ident == *handler_type {
                Some(parameter)
            } else {
                None
            }
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
                        if bounded_type.path.get_ident() == Some(handler_type) {
                            Self::row_type_from_fn_bounds(&predicate.bounds)
                        } else {
                            None
                        }
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
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let result = identifiers.allocate("result");
        let res = identifiers.allocate("res");
        let interrupt_result = identifiers.allocate("interrupt_result");
        let builder_ident = identifiers.allocate("builder");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let row_type = spec.row_type();
        let builder = crate::row_builder::RowBuilder::new(&spec.codegen_options, &builder_ident);
        let initialize_builder = builder.initialize(row_type);
        let build_row = builder.build();
        let builder_setter_calls = Self::builder_setter_calls(lockfile_entry, &builder, &result);
        let source_file = Self::source_file();
        if let Some(interrupt) = &spec.interrupt_handler {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #result = tokio::select! {
                        #res = #connection.query_one( #query, #params ) => #res,
                        #interrupt_result = &mut *#interrupt => return Err(#interrupt_result)
                    };
                    let #result = #result?;
                    #initialize_builder

                    #(#builder_setter_calls)*

                    Ok( #build_row )
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

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #result = #connection.query_one( #query, #params ).await?;
                    #initialize_builder

                    #(#builder_setter_calls)*

                    Ok( #build_row )
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
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let result = identifiers.allocate("result");
        let res = identifiers.allocate("res");
        let interrupt_result = identifiers.allocate("interrupt_result");
        let builder_ident = identifiers.allocate("builder");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let row_type = Self::option_row_type(spec);
        let builder = crate::row_builder::RowBuilder::new(&spec.codegen_options, &builder_ident);
        let initialize_builder = builder.initialize(row_type);
        let build_row = builder.build();
        let builder_setter_calls = Self::builder_setter_calls(lockfile_entry, &builder, &result);
        let source_file = Self::source_file();
        if let Some(interrupt) = &spec.interrupt_handler {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #result = tokio::select! {
                        #res = #connection.query_opt( #query, #params ) => #res,
                        #interrupt_result = &mut *#interrupt => return Err(#interrupt_result)
                    };
                    let Some(#result) = #result? else {
                        return Ok(None);
                    };
                    #initialize_builder

                    #(#builder_setter_calls)*

                    Ok(Some(#build_row))
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

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #result = #connection.query_opt( #query, #params ).await?;
                    let Some(#result) = #result else {
                        return Ok(None);
                    };
                    #initialize_builder

                    #(#builder_setter_calls)*

                    Ok(Some(#build_row))
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
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let rows = identifiers.allocate("rows");
        let result = identifiers.allocate("result");
        let res = identifiers.allocate("res");
        let interrupt_result = identifiers.allocate("interrupt_result");
        let builder_ident = identifiers.allocate("builder");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let (collector, handler, row_type) = Self::query_many_arguments(spec)?;
        let row_type = &row_type;
        let builder = crate::row_builder::RowBuilder::new(&spec.codegen_options, &builder_ident);
        let initialize_builder = builder.initialize(row_type);
        let build_row = builder.build();
        let builder_setter_calls = Self::builder_setter_calls(lockfile_entry, &builder, &result);
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

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #rows = tokio::select! {
                        #res = #connection.query_raw( #query, #params.iter().copied() ) => #res,
                        #interrupt_result = &mut *#interrupt => return Err(#interrupt_result)
                    }?;
                    let mut #rows = std::pin::pin!(#rows);
                    while let Some(#result) = tokio::select! {
                        #res = #rows.try_next() => #res?,
                        #interrupt_result = &mut *#interrupt => return Err(#interrupt_result)
                    } {
                        #initialize_builder

                        #(#builder_setter_calls)*

                        #collector = #handler(#collector, #build_row).await?;
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

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #rows = #connection.query_raw( #query, #params.iter().copied() ).await?;
                    let mut #rows = std::pin::pin!(#rows);
                    while let Some(#result) = #rows.try_next().await? {
                        #initialize_builder

                        #(#builder_setter_calls)*

                        #collector = #handler(#collector, #build_row).await?;
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
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let result = identifiers.allocate("result");
        let res = identifiers.allocate("res");
        let interrupt_result = identifiers.allocate("interrupt_result");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let source_file = Self::source_file();
        if let Some(interrupt) = &spec.interrupt_handler {
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #result = tokio::select! {
                        #res = #connection.execute( #query, #params ) => #res,
                        #interrupt_result = #interrupt => return Err(#interrupt_result.into())
                    }?;

                    Ok(#result)
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

                    let #params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[#(#parameters),*];
                    let #result = #connection.execute( #query, #params ).await?;

                    Ok(#result)
                }
            }
        }
    }
}

impl ConnectorCodeGenerator for TokioPostgresCodeGenerator {
    fn render_query_one(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_one_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function =
                crate::QueryRequirement::emit_stub_function(spec, sig, &Self::source_file())?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_query_opt(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_opt_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function =
                crate::QueryRequirement::emit_stub_function(spec, sig, &Self::source_file())?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_query_many(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_many_function(spec, sig, connection, lockfile_entry)?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function =
                crate::QueryRequirement::emit_stub_function(spec, sig, &Self::source_file())?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_execute(&self, spec: &QueryFunctionSpec) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::execute_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function =
                crate::QueryRequirement::emit_stub_function(spec, sig, &Self::source_file())?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }
}

#[cfg(feature = "sqlite")]
#[derive(Clone, Debug)]
struct RusqliteCodeGenerator {}

#[cfg(feature = "sqlite")]
impl RusqliteCodeGenerator {
    pub const fn new() -> Self {
        Self {}
    }

    fn builder_setter_calls(
        lockfile_entry: &LockfileEntry,
        builder: &crate::row_builder::RowBuilder<'_>,
        result: &Ident,
    ) -> Vec<TokenStream> {
        let mut calls = Vec::new();
        for field in lockfile_entry.result_set_shape() {
            let name = Ident::new(field.field_name(), Span::call_site());
            let index = LitInt::new(&field.field_index().to_string(), Span::call_site());
            calls.push(builder.setter(&name, &quote::quote! { #result.get(#index)? }));
        }
        calls
    }

    fn function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        query_body: &TokenStream,
        identifiers: &mut GeneratedIdentifiers,
    ) -> TokenStream {
        let source_file = TokioPostgresCodeGenerator::source_file();
        if let Some(interrupt) = &spec.interrupt_handler {
            let interrupt_message = identifiers.allocate("interrupt_message");
            let read_message = identifiers.allocate("read_message");
            let message = identifiers.allocate("message");
            let query_result = identifiers.allocate("query_result");
            let interrupt_error = identifiers.allocate("interrupt_error");
            let error = identifiers.allocate("error");
            let error_type = spec.error_type();
            quote::quote! {
                #sig
                {
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_FILE: &str = #source_file;
                    #[expect(dead_code)]
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();

                    let #interrupt_message = ::std::sync::Arc::new(::std::sync::Mutex::new(None));
                    let #read_message = ::std::sync::Arc::clone(&#interrupt_message);
                    #connection.progress_handler(1000, Some(move || {
                        match #interrupt.lock().expect("interrupt channel mutex lockable").try_recv() {
                            Ok(#message) => {
                                *#interrupt_message.lock().expect("interrupt message mutex lockable") = Some(#message);
                                true
                            }
                            Err(::std::sync::mpsc::TryRecvError::Empty) => false,
                            Err(::std::sync::mpsc::TryRecvError::Disconnected) => {
                                *#interrupt_message.lock().expect("interrupt message mutex lockable") = Some(#error_type::interrupt_disconnected());
                                true
                            }
                        }
                    }))?;

                    let #query_result: Result<_, #error_type> = (|| { #query_body })();
                    match #connection.progress_handler(1000, None::<fn() -> bool>) {
                        Ok(()) => match #read_message.lock().expect("interrupt message mutex lockable").take() {
                            Some(#interrupt_error) => Err(#interrupt_error),
                            None => #query_result,
                        },
                        Err(#error) => #error_type::interrupt_handler_cleanup_error(#error, #query_result),
                    }
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

                    #query_body
                }
            }
        }
    }

    fn query_one_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> TokenStream {
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let rows = identifiers.allocate("rows");
        let result = identifiers.allocate("result");
        let statement = identifiers.allocate("statement");
        let builder_ident = identifiers.allocate("builder");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let row_type = spec.row_type();
        let builder = crate::row_builder::RowBuilder::new(&spec.codegen_options, &builder_ident);
        let initialize_builder = builder.initialize(row_type);
        let build_row = builder.build();
        let setters = Self::builder_setter_calls(lockfile_entry, &builder, &result);
        let query_body = quote::quote! {
            let #params: &[&dyn ::rusqlite::ToSql] = &[#(#parameters),*];
            let mut #statement = #connection.prepare(#query)?;
            let mut #rows = #statement.query(#params)?;
            let Some(#result) = #rows.next()? else {
                return Err(::rusqlite::Error::QueryReturnedNoRows.into());
            };
            #initialize_builder
            #(#setters)*
            if #rows.next()?.is_some() {
                Err(::rusqlite::Error::QueryReturnedMoreThanOneRow.into())
            } else {
                Ok(#build_row)
            }
        };
        Self::function(spec, sig, connection, &query_body, &mut identifiers)
    }

    fn query_opt_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> TokenStream {
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let rows = identifiers.allocate("rows");
        let result = identifiers.allocate("result");
        let statement = identifiers.allocate("statement");
        let builder_ident = identifiers.allocate("builder");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let row_type = TokioPostgresCodeGenerator::option_row_type(spec);
        let builder = crate::row_builder::RowBuilder::new(&spec.codegen_options, &builder_ident);
        let initialize_builder = builder.initialize(row_type);
        let build_row = builder.build();
        let setters = Self::builder_setter_calls(lockfile_entry, &builder, &result);
        let query_body = quote::quote! {
            let #params: &[&dyn ::rusqlite::ToSql] = &[#(#parameters),*];
            let mut #statement = #connection.prepare(#query)?;
            let mut #rows = #statement.query(#params)?;
            if let Some(#result) = #rows.next()? {
                #initialize_builder
                #(#setters)*
                if #rows.next()?.is_some() {
                    Err(::rusqlite::Error::QueryReturnedMoreThanOneRow.into())
                } else {
                    Ok(Some(#build_row))
                }
            } else {
                Ok(None)
            }
        };
        Self::function(spec, sig, connection, &query_body, &mut identifiers)
    }

    fn query_many_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let rows = identifiers.allocate("rows");
        let result = identifiers.allocate("result");
        let statement = identifiers.allocate("statement");
        let builder_ident = identifiers.allocate("builder");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let (collector, handler, row_type) =
            TokioPostgresCodeGenerator::query_many_arguments(spec)?;
        let row_type = &row_type;
        let builder = crate::row_builder::RowBuilder::new(&spec.codegen_options, &builder_ident);
        let initialize_builder = builder.initialize(row_type);
        let build_row = builder.build();
        let setters = Self::builder_setter_calls(lockfile_entry, &builder, &result);
        let query_body = quote::quote! {
            let #params: &[&dyn ::rusqlite::ToSql] = &[#(#parameters),*];
            let mut #statement = #connection.prepare(#query)?;
            let mut #rows = #statement.query(#params)?;
            while let Some(#result) = #rows.next()? {
                #initialize_builder
                #(#setters)*
                #collector = #handler(#collector, #build_row)?;
            }
            Ok(#collector)
        };
        Ok(Self::function(
            spec,
            sig,
            connection,
            &query_body,
            &mut identifiers,
        ))
    }

    fn execute_function(
        spec: &QueryFunctionSpec,
        sig: &syn::Signature,
        connection: &Ident,
        lockfile_entry: &LockfileEntry,
    ) -> TokenStream {
        let mut identifiers = GeneratedIdentifiers::from(sig);
        let params = identifiers.allocate("params");
        let result = identifiers.allocate("result");

        let query = LitStr::new(lockfile_entry.execute_sql(), Span::call_site());
        let parameters = parameters(&spec.parameter_bindings, lockfile_entry);
        let query_body = quote::quote! {
            let #params: &[&dyn ::rusqlite::ToSql] = &[#(#parameters),*];
            let #result = #connection.execute(#query, #params)?;
            Ok(#result)
        };
        Self::function(spec, sig, connection, &query_body, &mut identifiers)
    }
}

#[cfg(feature = "sqlite")]
impl ConnectorCodeGenerator for RusqliteCodeGenerator {
    fn render_query_one(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_one_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = crate::QueryRequirement::emit_stub_function(
                spec,
                sig,
                &TokioPostgresCodeGenerator::source_file(),
            )?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_query_opt(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_opt_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = crate::QueryRequirement::emit_stub_function(
                spec,
                sig,
                &TokioPostgresCodeGenerator::source_file(),
            )?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_query_many(
        &self,
        spec: &QueryFunctionSpec,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::query_many_function(spec, sig, connection, lockfile_entry)?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = crate::QueryRequirement::emit_stub_function(
                spec,
                sig,
                &TokioPostgresCodeGenerator::source_file(),
            )?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }

    fn render_execute(&self, spec: &QueryFunctionSpec) -> Result<TokenStream, SqlFunCodegenError> {
        let item_fn = spec.item_fn();
        let sig = &item_fn.codegen_signature();
        let connection = spec.connection_ident();
        if let Some(lockfile_entry) = spec.lockfile_entry() {
            let function = Self::execute_function(spec, sig, connection, lockfile_entry);
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        } else {
            let function = crate::QueryRequirement::emit_stub_function(
                spec,
                sig,
                &TokioPostgresCodeGenerator::source_file(),
            )?;
            tracing::info!("generated source \n{}", function.to_string());
            Ok(function)
        }
    }
}

// Rust connector codegen must not parse, rewrite, inspect, or derive bind parameters
// from SQL text or placeholder syntax. Identity and order come from the lockfile
// contract, so SQL/query expansion can evolve independently of Rust value sources.
// execute_sql is only the final SQL literal; bind_parameters is the binding contract.
fn parameters(
    bindings: &crate::rust_syn::ParameterBindings,
    lockfile_entry: &LockfileEntry,
) -> Vec<TokenStream> {
    lockfile_entry
        .bind_parameters()
        .iter()
        .map(|parameter| bindings.expression(parameter.name()))
        .collect()
}

#[cfg(test)]
mod test_parameters {
    use super::parameters;
    use crate::rust_syn::ItemFunction;
    use ::sql_fun_core::lockfile::LockfileEntry;

    #[test]
    fn follows_lockfile_order_without_inspecting_sql() {
        let function = ItemFunction::new(syn::parse_quote! {
            fn query(#[parameter_source] dto: Dto, id: i64) {}
        });
        let entry: LockfileEntry = toml::from_str(
            r#"
            query_finger_print = "fixture"
            query_name = "query"
            execute_sql = "opaque SQL with no Rust placeholder syntax"
            bind_parameters = [
                { name = "name", parameter_index = 0 },
                { name = "id", parameter_index = 1 },
            ]
            result_set_shape = []
        "#,
        )
        .unwrap();
        let expressions = parameters(&function.parameter_bindings().unwrap(), &entry);
        assert_eq!(expressions[0].to_string(), "& dto . name ()");
        assert_eq!(expressions[1].to_string(), "& id");
    }
}
