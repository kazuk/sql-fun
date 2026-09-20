#![deny(missing_docs)]
//! procedural macro `sql-fun`
//!
mod errors;

use std::{
    env::var,
    fs::File,
    sync::{LazyLock, Mutex, Once},
};

use self::errors::SqlFunError;

use proc_macro2::TokenStream;

use sql_fun_codegen::{QueryFunctionBase, QueryFunctionForGenerate, QueryFunctionKind};
use sql_fun_core::{SqlFunArgs, SqlFunArgsError};
use syn::{ItemFn, LitStr, parse_macro_input};

static TRACING_INIT: Once = Once::new();

fn init_tracing() {
    TRACING_INIT.call_once(|| {
        if let Ok(path) = var("SQL_FUN_TRACE_FILE") {
            let file = File::create(path).unwrap();
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "off".into()),
                )
                .with_writer(Mutex::new(file))
                .try_init();
        } else {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "off".into()),
                )
                .try_init();
        }
    });
}

///
/// fill table/view fields to struct
///
#[proc_macro_attribute]
pub fn from_table(
    table_name: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    #[tracing::instrument()]
    fn from_table_impl(
        table_name: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        let table_name = parse_macro_input!(table_name as LitStr).value();

        match generate_table_struct(table_name, item.into()) {
            Ok(ts) => ts.into(),
            Err(err) => err.to_compile_error(),
        }
    }

    init_tracing();
    from_table_impl(table_name, item)
}

#[expect(clippy::unnecessary_wraps)]
fn generate_table_struct(
    _table_name: String,
    item: TokenStream,
) -> Result<TokenStream, SqlFunError> {
    Ok(item)
}

static SQLFUN_ARGS: LazyLock<Result<SqlFunArgs, SqlFunArgsError>> =
    LazyLock::new(SqlFunArgs::try_from_env);

#[tracing::instrument]
fn get_args() -> Result<&'static SqlFunArgs, proc_macro::TokenStream> {
    match &*SQLFUN_ARGS {
        Ok(v) => Ok(v),
        Err(e) => {
            let message = format!("{e}");
            let compile_error = syn::Error::new_spanned("", message).into_compile_error();
            Err(compile_error.into())
        }
    }
}

#[proc_macro_attribute]
#[doc = include_str!("attr_sql_query_one.md")]
pub fn sql_query_one(
    sql_attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    #[tracing::instrument(skip(sql_attr, item))]
    fn sql_query_one_impl(
        sql_attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        let args = match get_args() {
            Ok(args) => args,
            Err(e) => return e,
        };
        let item_fn = parse_macro_input!(item as ItemFn);
        let query_one_fn = QueryFunctionForGenerate::try_from_macro_input(
            args,
            sql_attr.into(),
            item_fn,
            QueryFunctionKind::QueryOne,
        )
        .and_then(sql_fun_codegen::QueryFunctionForGenerate::code);
        match query_one_fn {
            Ok(query_one) => query_one.into(),
            Err(err) => err.into_compile_error().into(),
        }
    }

    init_tracing();
    sql_query_one_impl(sql_attr, item)
}

#[proc_macro_attribute]
#[doc = include_str!("attr_sql_query_opt.md")]
pub fn sql_query_opt(
    sql_attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    #[tracing::instrument(skip(sql_attr, item))]
    fn sql_query_opt_impl(
        sql_attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        let args = match get_args() {
            Ok(args) => args,
            Err(e) => return e,
        };
        let item_fn = parse_macro_input!(item as ItemFn);
        let query_opt_fn = QueryFunctionForGenerate::try_from_macro_input(
            args,
            sql_attr.into(),
            item_fn,
            QueryFunctionKind::QueryOpt,
        )
        .and_then(sql_fun_codegen::QueryFunctionForGenerate::code);
        match query_opt_fn {
            Ok(query_opt) => query_opt.into(),
            Err(err) => err.into_compile_error().into(),
        }
    }

    init_tracing();
    sql_query_opt_impl(sql_attr, item)
}

#[proc_macro_attribute]
#[doc = include_str!("attr_sql_query_many.md")]
pub fn sql_query_many(
    sql_attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    #[tracing::instrument(skip(sql_attr, item))]
    fn sql_query_many_impl(
        sql_attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        let args = match get_args() {
            Ok(args) => args,
            Err(e) => return e,
        };
        let item_fn = parse_macro_input!(item as ItemFn);
        let query_many_fn = QueryFunctionForGenerate::try_from_macro_input(
            args,
            sql_attr.into(),
            item_fn,
            QueryFunctionKind::QueryMany,
        )
        .and_then(sql_fun_codegen::QueryFunctionForGenerate::code);
        match query_many_fn {
            Ok(query_opt) => query_opt.into(),
            Err(err) => err.into_compile_error().into(),
        }
    }

    init_tracing();
    sql_query_many_impl(sql_attr, item)
}

/// This macro attribute is used to specify the SQL statement to be executed
///
/// ```no_run
/// use tokio_postgres::{Client as PgClient, Error as PgError};
///
/// #[sql_fun::sql_statement("create table users(id serial primary key, name text);")]
/// async fn create_table_users(client: &PgClient) -> Result<u64, PgError> {}
/// ```
///
/// please set connector in Cargo.toml
///
/// ```toml
/// [package.metadata.database]
/// connector = "tokio-postgres"
/// ```
///
/// `sql_fun` supports only the `tokio-postgres` for database connector(currently).
///
#[proc_macro_attribute]
pub fn sql_statement(
    sql_attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    #[tracing::instrument(skip(sql_attr, item))]
    fn sql_statement_impl(
        sql_attr: proc_macro::TokenStream,
        item: proc_macro::TokenStream,
    ) -> proc_macro::TokenStream {
        let args = match get_args() {
            Ok(args) => args,
            Err(e) => return e,
        };
        let item_fn = parse_macro_input!(item as ItemFn);
        let query_many_fn = QueryFunctionForGenerate::try_from_macro_input(
            args,
            sql_attr.into(),
            item_fn,
            QueryFunctionKind::Execute,
        )
        .and_then(sql_fun_codegen::QueryFunctionForGenerate::code);
        match query_many_fn {
            Ok(query_opt) => query_opt.into(),
            Err(err) => err.into_compile_error().into(),
        }
    }

    init_tracing();
    sql_statement_impl(sql_attr, item)
}
