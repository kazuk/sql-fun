mod execute;
mod query_many;
mod query_one;
mod query_opt;

use quote::quote;
use syn::{Ident, ItemFn, Path};

use crate::{errors::SqlFunError, sql_ast::get_result_set_column_names};

#[derive(derive_builder::Builder)]
pub struct TokioPostgresCodeGenerator {
    sql_str: String,
    #[builder(default, setter(strip_option))]
    row_type: Option<Path>,
    #[builder(default, setter(strip_option))]
    error_type: Option<Path>,
    client_arg_name: Ident,
    #[builder(default, setter(strip_option))]
    collector_arg: Option<Ident>,
    #[builder(default, setter(strip_option))]
    handler_arg: Option<Ident>,
    placeholders: Vec<String>,
    #[builder(default)]
    result_columns: Vec<String>,
}

/// Extract placeholders from the SQL statement
///
/// Placeholders are specified with `${name}`
/// and replaced with $1, $2, $3, ...
///
fn extract_placeholders(sql_str: &str) -> (Vec<String>, String) {
    let mut placeholder_names = Vec::new();

    let mut start = 0;
    while let Some((placeholder, remains)) = sql_str[start..].find("${").map(|start| {
        let end = sql_str[start..].find('}').map(|end| start + end).unwrap();
        (&sql_str[start + 2..end], end + 1)
    }) {
        if !placeholder_names.contains(&placeholder.to_string()) {
            placeholder_names.push(placeholder.to_string());
        }
        start = remains;
    }

    let mut sql_str = sql_str.to_string();
    for (i, name) in placeholder_names.iter().enumerate() {
        let placeholder_index = format!("${}", i + 1);
        let placeholder_pattern = format!("${{{name}}}");
        sql_str = sql_str.replace(&placeholder_pattern, &placeholder_index);
    }

    (placeholder_names, sql_str)
}

fn validate_function_arguments(
    placeholders: &Vec<String>,
    function_ast: &ItemFn,
) -> Result<syn::Ident, SqlFunError> {
    let client_arg = function_ast
        .sig
        .inputs
        .first()
        .ok_or_else(|| SqlFunError::custom("function must have a client argument"))?;
    let client_arg_name = match client_arg {
        syn::FnArg::Typed(pat_type) => {
            let pat = pat_type.pat.as_ref();
            match pat {
                syn::Pat::Ident(ident) => ident.ident.clone(),
                _ => {
                    return Err(SqlFunError::custom("client argument must be an identifier"));
                }
            }
        }
        syn::FnArg::Receiver(_) => {
            return Err(SqlFunError::custom(
                "client argument must be a typed argument",
            ));
        }
    };
    for name in placeholders {
        let mut found = false;
        for arg in &function_ast.sig.inputs {
            if let syn::FnArg::Typed(pat_type) = arg {
                let pat = pat_type.pat.as_ref();
                if let syn::Pat::Ident(ident) = pat
                    && ident.ident == *name
                {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return Err(SqlFunError::custom(&format!(
                "binding parameter {name} not found",
            )));
        }
    }
    Ok(client_arg_name)
}

impl TokioPostgresCodeGeneratorBuilder {
    pub fn set_sql(
        &mut self,
        function_ast: &ItemFn,
        sql_string: &str,
    ) -> Result<&mut Self, SqlFunError> {
        let (placeholders, sql_str) = extract_placeholders(sql_string);
        let sql_ast = pg_query::parse(&sql_str)?;
        let client_arg_name = validate_function_arguments(&placeholders, function_ast)?;

        self.sql_str(sql_str);
        self.placeholders(placeholders);
        self.client_arg_name(client_arg_name);
        self.result_columns(get_result_set_column_names(&sql_ast)?);

        Ok(self)
    }
}

impl TokioPostgresCodeGenerator {
    pub fn builder() -> TokioPostgresCodeGeneratorBuilder {
        TokioPostgresCodeGeneratorBuilder::default()
    }
}

impl TokioPostgresCodeGenerator {
    fn generate_check_column_existing(index: usize, column: &String) -> proc_macro2::TokenStream {
        let error_message = format!("column name mismatch: expected {column} but ");
        let check_column_name = quote! {
            if #column != columns[#index].name() {
                let mut msg = String::from(#error_message);
                msg.push_str(columns[#index].name());
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, msg ))?;
            }
        };
        check_column_name
    }
}

impl TokioPostgresCodeGenerator {
    fn generate_set_filed_value(index: usize, column: &str) -> proc_macro2::TokenStream {
        let ident_column = syn::Ident::new(column, proc_macro::Span::call_site().into());
        let set_field_value = quote! {
            row_builder.#ident_column( row.try_get(#index)? );
        };
        set_field_value
    }
}
