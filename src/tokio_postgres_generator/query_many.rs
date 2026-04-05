use super::TokioPostgresCodeGenerator;
use quote::quote;

impl TokioPostgresCodeGenerator {
    pub fn generate_query_many(self) -> proc_macro2::TokenStream {
        let Self {
            row_type,
            error_type: _,
            client_arg_name,
            sql_str,
            placeholders,
            result_columns,
            collector_arg,
            handler_arg,
        } = self;
        let collector_arg = collector_arg.expect("query many must have collector");
        let handler_arg = handler_arg.expect("query many must have handelr");
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
        let sql_execution_code = quote! {
            {
                use std::sync::atomic::{AtomicBool, Ordering};
                use futures_util::{pin_mut, TryStreamExt};

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
                let it = #client_arg_name.query_raw(#sql_str, &[#(#parameters),*]).await?;
                pin_mut!(it);
                while let Some(row) = it.try_next().await? {
                    let mut row_builder = #row_type::builder();
                    #(#set_field_values)*;
                    let row = row_builder.build()?;
                    #collector_arg = #handler_arg( #collector_arg,row ).await?;
                }
                Ok(collector)
            }
        };
        sql_execution_code
    }
}
