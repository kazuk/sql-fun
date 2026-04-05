use super::TokioPostgresCodeGenerator;
use quote::quote;

impl TokioPostgresCodeGenerator {
    pub fn generate_execute(self) -> proc_macro2::TokenStream {
        let Self {
            sql_str,
            client_arg_name,
            placeholders,
            ..
        } = self;

        let mut parameters = Vec::new();
        for placeholder in &placeholders {
            let ident = syn::Ident::new(placeholder, proc_macro::Span::call_site().into());
            let parameter = quote! { &#ident };
            parameters.push(parameter);
        }

        let sql_execution_code = quote! {
            {
                use std::sync::atomic::{AtomicBool, Ordering};
                static CHECKED: AtomicBool = AtomicBool::new(false);
                if !CHECKED.load(Ordering::Relaxed) {
                    let statement = #client_arg_name.prepare(#sql_str).await?;
                    CHECKED.store(true, Ordering::Relaxed)
                }

                let result = #client_arg_name.execute(#sql_str, &[#(#parameters),*]).await?;
                Ok(result)
            }
        };
        sql_execution_code
    }
}
