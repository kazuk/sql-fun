use std::collections::HashSet;

use ::proc_macro2::Span;
use ::syn::{Ident, Signature, visit::Visit};

/// Allocates runtime bindings without shadowing names in the emitted signature.
/// One instance belongs to one final function, including its generated helpers.
pub(crate) struct GeneratedIdentifiers {
    used: HashSet<String>,
}

impl<'ast> Visit<'ast> for GeneratedIdentifiers {
    fn visit_ident(&mut self, ident: &'ast Ident) {
        let spelling = ident.to_string();
        self.used
            .insert(spelling.strip_prefix("r#").unwrap_or(&spelling).to_owned());
    }
}

impl From<&Signature> for GeneratedIdentifiers {
    fn from(signature: &Signature) -> Self {
        let mut identifiers = Self {
            used: HashSet::new(),
        };
        identifiers.visit_signature(signature);
        identifiers
    }
}

impl GeneratedIdentifiers {
    pub(crate) fn allocate(&mut self, role: &str) -> Ident {
        let base = format!("__sql_fun_{role}");
        let mut candidate = base.clone();
        let mut suffix = 0;
        while !self.used.insert(candidate.clone()) {
            suffix += 1;
            candidate = format!("{base}_{suffix}");
        }
        Ident::new(&candidate, Span::call_site())
    }
}

#[cfg(test)]
mod test_generated_identifiers {
    use super::GeneratedIdentifiers;

    #[test]
    fn reserves_generated_names_and_skips_occupied_suffixes() {
        let signature = syn::parse_quote! {
            fn query(__sql_fun_rows: (), __sql_fun_rows_1: (), __sql_fun_rows_3: ())
        };
        let mut identifiers = GeneratedIdentifiers::from(&signature);
        assert_eq!(identifiers.allocate("params"), "__sql_fun_params");
        assert_eq!(identifiers.allocate("rows"), "__sql_fun_rows_2");
        assert_eq!(identifiers.allocate("rows"), "__sql_fun_rows_4");
    }

    #[test]
    fn raw_identifiers_reserve_their_non_raw_spelling() {
        let signature = syn::parse_quote! {
            fn query(r#__sql_fun_rows: (), r#__sql_fun_rows_1: ())
        };
        let mut identifiers = GeneratedIdentifiers::from(&signature);
        assert_eq!(identifiers.allocate("rows"), "__sql_fun_rows_2");
    }

    #[test]
    fn visits_patterns_generics_types_and_where_clauses() {
        let signature = syn::parse_quote! {
            fn query<const __sql_fun_rows: usize>(
                (__sql_fun_rows_1, other): (__sql_fun_rows_2, usize)
            ) where __sql_fun_rows_3: Trait
        };
        let mut identifiers = GeneratedIdentifiers::from(&signature);
        assert_eq!(identifiers.allocate("rows"), "__sql_fun_rows_4");
    }

    #[test]
    fn allocation_is_deterministic_and_local_to_each_function() {
        let signature = syn::parse_quote! { fn query(__sql_fun_rows: ()) };
        let mut first = GeneratedIdentifiers::from(&signature);
        let mut second = GeneratedIdentifiers::from(&signature);
        for role in ["rows", "params", "rows", "builder"] {
            assert_eq!(first.allocate(role), second.allocate(role));
        }
        let mut next_function = GeneratedIdentifiers::from(&signature);
        assert_eq!(next_function.allocate("rows"), "__sql_fun_rows_1");
    }
}
