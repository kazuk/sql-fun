use ::proc_macro2::TokenStream;
use ::sql_fun_core::CodegenOptions;
use ::syn::{Ident, Type};

/// Emits the independent setter and build contracts for either connector.
pub(crate) struct RowBuilder<'a> {
    options: &'a CodegenOptions,
    ident: Ident,
}

impl<'a> RowBuilder<'a> {
    pub(crate) fn new(options: &'a CodegenOptions, ident: &Ident) -> Self {
        Self {
            options,
            ident: ident.clone(),
        }
    }

    pub(crate) fn initialize(&self, row_type: &Type) -> TokenStream {
        let builder = &self.ident;
        if self.options.builder_setter().is_mutable() {
            quote::quote! { let mut #builder = #row_type::builder(); }
        } else {
            quote::quote! { let #builder = #row_type::builder(); }
        }
    }

    pub(crate) fn setter(&self, field: &Ident, value: &TokenStream) -> TokenStream {
        let builder = &self.ident;
        if self.options.builder_setter().is_mutable() {
            quote::quote! { #builder.#field(#value); }
        } else {
            quote::quote! { let #builder = #builder.#field(#value); }
        }
    }

    pub(crate) fn build(&self) -> TokenStream {
        let builder = &self.ident;
        if self.options.builder_build_fallible() {
            quote::quote! { #builder.build()? }
        } else {
            quote::quote! { #builder.build() }
        }
    }
}
