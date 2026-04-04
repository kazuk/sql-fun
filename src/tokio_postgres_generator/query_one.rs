use super::TokioPostgresCodeGenerator;
use quote::quote;

impl TokioPostgresCodeGenerator {
    pub fn generate_query_one(self) -> proc_macro2::TokenStream {
        let Self {
            row_type,
            error_type,
            client_arg_name,
            sql_str,
            placeholders,
            result_columns,
            collector_arg: _,
            handler_arg: _,
        } = self;
        let error_type = error_type.unwrap();

        let mut parameters = Vec::new();
        for placeholder in &placeholders {
            let ident = syn::Ident::new(placeholder, proc_macro::Span::call_site().into());
            let parameter = quote! { &#ident };
            parameters.push(parameter);
        }

        let mut check_column_names = Vec::new();
        let mut set_field_values = Vec::new();
        for (index, column) in result_columns.iter().enumerate() {
            check_column_names.push(Self::generate_check_column_existing(index, column));
            set_field_values.push(Self::generate_set_filed_value(index, column));
        }

        let result_field_count = check_column_names.len();

        quote! {
            {
                use std::sync::atomic::{AtomicBool, Ordering};
                static CHECKED: AtomicBool = AtomicBool::new(false);
                if !CHECKED.load(Ordering::Relaxed) {
                    let statement = #client_arg_name.prepare(#sql_str).await?;
                    let columns = statement.columns();
                    if columns.len() != #result_field_count {
                        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "result set column count missmatch"))?;
                    }
                    #(#check_column_names)*;
                    CHECKED.store(true, Ordering::Relaxed);
                }
                let row = #client_arg_name.query_one(#sql_str, &[#(#parameters),*]).await?;
                let mut row_builder = #row_type::builder();
                #(#set_field_values)*;
                Ok::<_, #error_type>(row_builder.build()?)
            }
        }
    }
}
