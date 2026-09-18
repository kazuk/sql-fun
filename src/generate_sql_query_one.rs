use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

use crate::{Connector, SqlFunError, TokioPostgresCodeGenerator, get_result_type_args};
use sql_fun_core::{SqlFunArgs, SqlFunMetadataCollection};

/// generate `sql_query_one`
pub fn generate_sql_query_one(
    sql_str: &str,
    item: TokenStream,
) -> Result<TokenStream, SqlFunError> {
    let args = SqlFunArgs::try_from_env()?;
    let metadata_collection = SqlFunMetadataCollection::load_from(args.metadata_file()?)?;
    let metadata = metadata_collection.get("default").unwrap();
    let _connector = Connector::get_connector(metadata)?;

    let mut code_generator_builder = TokioPostgresCodeGenerator::builder();
    let mut function_ast: ItemFn = syn::parse2(item)?;

    if function_ast.sig.asyncness.is_none() {
        return Err(SqlFunError::custom("function must be async"));
    }

    code_generator_builder.set_sql(&function_ast, sql_str)?;

    let (row_type, error_type) = get_result_type_args(&function_ast)?;

    code_generator_builder.row_type(row_type);
    code_generator_builder.error_type(error_type);

    if function_ast.sig.asyncness.is_none() {
        return Err(SqlFunError::custom("function must be async"));
    }
    let generator = code_generator_builder.build()?;
    let sql_execution_code = generator.generate_query_one();

    function_ast
        .block
        .stmts
        .insert(0, syn::parse2(sql_execution_code)?);
    Ok(quote! { #function_ast })
}
