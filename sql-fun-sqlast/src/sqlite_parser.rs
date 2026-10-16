//! Optional precise type locations on the locked parser's grammar.
//!
//! The generated modules adapt immutable upstream parser/lexer sources. Native
//! ASTs, errors and token kinds remain the existing dependency's representations.
//! Upstream style lints are suppressed here to keep the pinned sources immutable.
#![allow(
    missing_docs,
    dead_code,
    unexpected_cfgs,
    clippy::all,
    clippy::missing_const_for_fn
)]

pub(crate) use ::turso_parser::{ast, error, token};
mod lexer {
    include!(concat!(env!("OUT_DIR"), "/lexer.rs"));
}
// Keep the pinned third-party source unchanged. Remove this exception with the
// vendored workaround when the upstream Turso fix is incorporated (issue #466).
#[allow(clippy::disallowed_methods)]
mod parser {
    include!(concat!(env!("OUT_DIR"), "/parser.rs"));
}

pub(crate) use self::parser::{Parser, TypeSpan};
type Result<T, E = error::Error> = std::result::Result<T, E>;
