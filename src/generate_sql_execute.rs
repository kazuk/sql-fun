use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

use crate::{Connector, SqlFunError, TokioPostgresCodeGenerator};

/// generate for sql_execture
pub fn generate_sql_execute(
    sql_str: String,
    item: TokenStream,
) -> Result<TokenStream, SqlFunError> {
    let _connector = Connector::get_connector()?;
    let mut code_generator_builder = TokioPostgresCodeGenerator::builder();
    let mut function_ast: ItemFn = syn::parse2(item)?;

    if function_ast.sig.asyncness.is_none() {
        return Err(SqlFunError::custom("function must be async"));
    }

    code_generator_builder.set_sql(&function_ast, &sql_str)?;

    let generator = code_generator_builder.build()?;
    let sql_execution_code = generator.generate_execute();
    function_ast
        .block
        .stmts
        .insert(0, syn::parse2(sql_execution_code)?);
    Ok(TokenStream::from(quote! { #function_ast }))
}
