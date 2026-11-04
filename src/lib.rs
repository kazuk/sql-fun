#![deny(missing_docs)]
//! procedural macro `sql-fun`
//!
use std::{
    env::var,
    fs::File,
    sync::{LazyLock, Mutex, Once},
};

use sql_fun_codegen::{QueryFunctionBase, QueryFunctionForGenerate, QueryFunctionKind};
use sql_fun_core::{SqlFunArgs, SqlFunArgsError};
use syn::{ItemFn, parse_macro_input};

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

/// Execute a statement and return its affected-row count in a `Result`.
/// PostgreSQL uses an async function returning `u64`; SQLite uses a synchronous
/// function returning `usize` and requires the `sqlite` feature.
///
/// ```no_run
/// use tokio_postgres::{Client as PgClient, Error as PgError};
///
/// #[sql_fun::sql_statement(sql = "create table users(id serial primary key, name text);")]
/// async fn create_table_users(client: &PgClient) -> Result<u64, PgError> {}
/// ```
///
/// Configure a named datasource with an engine, dialect and connector in package
/// metadata. See the [SQL Functions reference](https://kazuk.github.io/sql-fun/sql_functions.html)
/// for attributes, signatures, bindings and interruption contracts. This macro
/// does not build rows; use a query macro to collect a RETURNING result.
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
