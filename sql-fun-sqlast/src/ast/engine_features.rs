use crate::{
    ast::ConstraintExtras,
    syn::{self, Opt},
};
use ::sql_fun_core::IVec;

impl ConstraintExtras {
    pub(crate) fn explicit_enforcement(
        value: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Option<bool> {
        if !matches!(
            value.get_contype().as_inner(),
            Some(syn::ConstrType::ConstrCheck | syn::ConstrType::ConstrForeign)
        ) {
            return None;
        }
        let mut depth = 0;
        for token in tokens
            .iter()
            .filter(|token| token.get_start() >= value.get_location())
        {
            match token.get_token().as_inner() {
                Some(syn::Token::Ascii40) => depth += 1,
                Some(syn::Token::Ascii41) if depth == 0 => break,
                Some(syn::Token::Ascii41) => depth -= 1,
                Some(syn::Token::Ascii44) if depth == 0 => break,
                Some(syn::Token::Enforced) if depth == 0 => return Some(value.get_is_enforced()),
                _ => {}
            }
        }
        None
    }
}
