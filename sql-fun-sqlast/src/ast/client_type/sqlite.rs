use crate::{
    ast::client_type::{
        ClientTypeHint, ClientTypeName, invalid,
        token::{TypeToken, TypeTokens},
    },
    error::AnalysisError,
};
use ::combine::{Parser, many, many1};

pub(super) fn parse(tokens: &TypeTokens) -> Result<ClientTypeHint, AnalysisError> {
    let ((name, arrays), remaining) = (
        many1::<Vec<_>, _, _>(TypeToken::name(true)),
        many::<Vec<_>, _, _>(TypeToken::punctuation("[").skip(TypeToken::punctuation("]"))),
    )
        .parse(tokens.as_slice())
        .map_err(|_| invalid("invalid SQLite client type hint syntax"))?;
    if let Some(token) = remaining.first() {
        return if token.is_symbol("(") {
            Ok(ClientTypeHint::unsupported_modifier())
        } else {
            Err(invalid("unexpected tokens after SQLite type name"))
        };
    }
    Ok(ClientTypeHint::new(
        ClientTypeName::Sqlite(name),
        !arrays.is_empty(),
    ))
}
