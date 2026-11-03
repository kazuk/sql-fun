use crate::{
    names::FullName,
    sem::{
        AnalysisError, TypeReference,
        client_type::{
            invalid,
            token::{TypeToken, TypeTokens},
            typmod,
        },
    },
};
use ::combine::{Parser, many, many1};

pub(super) fn parse(tokens: &TypeTokens) -> Result<TypeReference, AnalysisError> {
    let ((name, arrays), remaining) = (
        many1::<Vec<_>, _, _>(TypeToken::name(true)),
        many::<Vec<_>, _, _>(TypeToken::punctuation("[").skip(TypeToken::punctuation("]"))),
    )
        .parse(tokens.as_slice())
        .map_err(|_| invalid("invalid SQLite client type hint syntax"))?;
    if let Some(token) = remaining.first() {
        return Err(if token.is_symbol("(") {
            typmod()
        } else {
            invalid("unexpected tokens after SQLite type name")
        });
    }
    Ok(TypeReference::concrete_type_ref(
        &FullName::from_local_name(&name.join(" ")),
        !arrays.is_empty(),
    ))
}
