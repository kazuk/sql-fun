use super::token::{TypeToken, TypeTokens};
use super::{invalid, typmod};
use crate::sem::{AnalysisError, FullName, TypeReference};
use ::combine::{Parser, many, many1};

pub(super) fn parse(tokens: &TypeTokens) -> Result<TypeReference, AnalysisError> {
    let ((name, arrays), remaining) = (
        many1::<Vec<_>, _, _>(TypeToken::name(true)),
        many::<Vec<_>, _, _>(TypeToken::punctuation("[").skip(TypeToken::punctuation("]"))),
    )
        .parse(tokens.as_slice())
        .map_err(|error| invalid(&format!("invalid SQLite type syntax: {error:?}")))?;
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
