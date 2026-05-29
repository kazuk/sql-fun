#![deny(missing_docs)]
//! procedural macro `sql-fun`
//!
mod connector;
mod errors;
mod generate_sql_execute;
mod generate_sql_query_one;
mod generate_sql_query_opt;
mod generate_sqy_query_many;
mod rust_ast;
mod sql_ast;
mod tokio_postgres_generator;

use self::{
    connector::Connector,
    errors::SqlFunError,
    generate_sql_execute::generate_sql_execute,
    generate_sql_query_one::generate_sql_query_one,
    generate_sql_query_opt::generate_sql_query_opt,
    generate_sqy_query_many::generate_sql_query_many,
    rust_ast::{
        get_arg_name, get_generic_type_bounds, get_last_parameter_name, get_last_parameter_type,
        get_last_parameter_type_from_fn_trait, get_result_types,
    },
    tokio_postgres_generator::{
        TokioPostgresCodeGenerator, TokioPostgresCodeGeneratorBuilderError,
    },
};

use proc_macro2::TokenStream;

use syn::{Ident, ItemFn, LitStr, Path, Type, parse_macro_input};

///
/// fill table/view fields to struct
///
#[proc_macro_attribute]
pub fn from_table(
    table_name: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let table_name = parse_macro_input!(table_name as LitStr).value();

    match generate_table_struct(table_name, item.into()) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error(),
    }
}

#[expect(clippy::unnecessary_wraps)]
fn generate_table_struct(
    _table_name: String,
    item: TokenStream,
) -> Result<TokenStream, SqlFunError> {
    Ok(item)
}

#[proc_macro_attribute]
#[doc = include_str!("attr_sql_query_one.md")]
pub fn sql_query_one(
    sql_attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let sql_str = parse_macro_input!(sql_attr as LitStr).value();

    match generate_sql_query_one(&sql_str, item.into()) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error(),
    }
}

#[proc_macro_attribute]
#[doc = include_str!("attr_sql_query_opt.md")]
pub fn sql_query_opt(
    sql_attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let sql_str = parse_macro_input!(sql_attr as LitStr).value();

    match generate_sql_query_opt(&sql_str, item.into()) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error(),
    }
}

#[proc_macro_attribute]
#[doc = include_str!("attr_sql_query_many.md")]
pub fn sql_query_many(
    sql_attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let sql_str = parse_macro_input!(sql_attr as LitStr).value();

    match generate_sql_query_many(&sql_str, item.into()) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error(),
    }
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
    let sql_str = parse_macro_input!(sql_attr as LitStr).value();

    match generate_sql_execute(&sql_str, item.into()) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error(),
    }
}

fn get_result_type_args(item_fn: &ItemFn) -> Result<(Path, Path), SqlFunError> {
    let Some((value_type, error_type)) = get_result_types(item_fn) else {
        return Err(SqlFunError::custom("function must returns Result<T,E>"));
    };
    let Type::Path(value_path) = value_type else {
        return Err(SqlFunError::custom(
            "must T is concrete type on Result<T,E> ",
        ));
    };
    let Type::Path(error_path) = error_type else {
        return Err(SqlFunError::custom(
            "must E is concrete type on Result<T,E>",
        ));
    };
    Ok((value_path.path.clone(), error_path.path.clone()))
}

fn handler_type_name(function_ast: &ItemFn) -> Result<String, SqlFunError> {
    let ident = get_last_parameter_type(function_ast)
        .ok_or_else(|| SqlFunError::custom("function must have a handler argument"))?;
    Ok(ident.to_string())
}

fn handler_arg_name(function_ast: &ItemFn) -> Result<Ident, SqlFunError> {
    get_last_parameter_name(function_ast)
        .ok_or_else(|| SqlFunError::custom("function must have a handler argument"))
}

fn collector_arg_name(function_ast: &ItemFn) -> Result<Ident, SqlFunError> {
    let index = function_ast.sig.inputs.len() - 2;
    function_ast
        .sig
        .inputs
        .iter()
        .nth(index)
        .and_then(get_arg_name)
        .ok_or_else(|| SqlFunError::custom("invalid collector arg"))
}
