use proc_macro2::TokenStream;
use syn::ItemFn;

use crate::{
    Connector, SqlFunError, TokioPostgresCodeGenerator, collector_arg_name,
    get_generic_type_bounds, get_last_parameter_type_from_fn_trait, handler_arg_name,
    handler_type_name,
};
use quote::quote;

/// generate sql_query_many
pub fn generate_sql_query_many(
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

    code_generator_builder.handler_arg(handler_arg_name(&function_ast)?);
    code_generator_builder.collector_arg(collector_arg_name(&function_ast)?);
    let handler_type = handler_type_name(&function_ast)?;

    let Some(handler_ty) = get_generic_type_bounds(&function_ast, &handler_type) else {
        return Err(SqlFunError::custom("failed to get bounds for handler"));
    };

    code_generator_builder.row_type(
        get_last_parameter_type_from_fn_trait(handler_ty)
            .ok_or_else(|| SqlFunError::custom("failed to get row type"))?,
    );

    if function_ast.sig.asyncness.is_none() {
        return Err(SqlFunError::custom("function must be async"));
    }

    let generator = code_generator_builder.build()?;
    let sql_execution_code = generator.generate_query_many();
    function_ast
        .block
        .stmts
        .push(syn::parse2(sql_execution_code)?);
    Ok(TokenStream::from(quote! { #function_ast }))
}
