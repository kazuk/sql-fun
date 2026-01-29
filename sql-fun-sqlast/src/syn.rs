#![doc = include_str!("syn.md")]

mod generated;
mod syn_ext;

use itertools::Itertools;
use sql_fun_core::IVec;

use crate::StringSpan;

pub use self::{
    generated::*,
    syn_ext::{ListOpt, Opt},
};

impl ScanToken {
    /// return true when self is identifiter
    pub fn is_ident(&self) -> bool {
        if let Some(tok) = self.get_token().as_inner() {
            matches!(tok, crate::syn::Token::Ident)
        } else {
            false
        }
    }

    /// Return true when self is '.'
    pub fn is_dot(&self) -> bool {
        if let Some(tok) = self.get_token().as_inner() {
            matches!(tok, crate::syn::Token::Ascii46)
        } else {
            false
        }
    }

    /// Return true when self is comment
    pub fn is_comment(&self) -> bool {
        if let Some(tok) = self.get_token().as_inner() {
            matches!(tok, crate::syn::Token::Comment)
        } else {
            false
        }
    }
}

impl TypeName {
    /// convert `TypeName` to `full_name` string
    #[must_use]
    pub fn full_name(&self) -> std::string::String {
        let Some(names) = self.get_names().as_inner() else {
            return std::string::String::from("");
        };
        names
            .iter()
            .filter_map(|v| v.as_string().get_sval())
            .join(".")
    }

    /// get type span
    pub fn get_name_span(&self, tokens: &IVec<ScanToken>) -> StringSpan {
        let location = self.get_location();
        if location <= 0 {
            panic!("invalid location {self:?}")
        }
        identifier_many_dot_identifier(tokens, location)
    }
}

/// get span for identifier *( dot identifier )
fn identifier_many_dot_identifier(tokens: &IVec<ScanToken>, location: i32) -> StringSpan {
    let (start_token_index, tok) = tokens
        .iter()
        .find_position(|t| t.get_start() == location)
        .unwrap();
    let mut result_span = StringSpan::from_scan_token(tok);
    let names_token = &mut tokens.as_slice()[start_token_index..].iter();
    let mut expect_ident = true;
    for tok in names_token {
        if tok.is_comment() {
            continue;
        }
        if expect_ident {
            if tok.is_ident() {
                result_span.extend(&StringSpan::from_scan_token(tok));
                expect_ident = false;
                continue;
            } else {
                break;
            }
        } else if tok.is_dot() {
            result_span.extend(&StringSpan::from_scan_token(tok));
            expect_ident = true;
            continue;
        } else {
            break;
        }
    }
    result_span
}

impl FuncCall {
    /// get `funcname` span
    pub fn get_funcname_span(&self, tokens: &IVec<ScanToken>) -> StringSpan {
        let location = self.get_location();
        if location <= 0 {
            panic!("invalid location {self:?}")
        }
        identifier_many_dot_identifier(tokens, location)
    }
}
