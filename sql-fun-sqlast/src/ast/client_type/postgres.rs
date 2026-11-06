use crate::{
    ast::client_type::{
        ClientBuiltin, ClientTypeHint, ClientTypeName, ClientTypePath, invalid,
        token::{TypeToken, TypeTokens},
    },
    error::AnalysisError,
};
use ::combine::{Parser, attempt, choice, many, optional};

pub(super) fn parse(tokens: &TypeTokens) -> Result<ClientTypeHint, AnalysisError> {
    if tokens.interval_modifier() {
        return Ok(ClientTypeHint::unsupported_modifier());
    }
    let ((name, array), remaining) = (simple_name(), array_suffix())
        .parse(tokens.as_slice())
        .map_err(|_| invalid("invalid PostgreSQL client type hint syntax"))?;
    if let Some(token) = remaining.first() {
        return if token.is_symbol("(") {
            Ok(ClientTypeHint::unsupported_modifier())
        } else {
            Err(invalid("unexpected tokens after PostgreSQL type name"))
        };
    }
    Ok(ClientTypeHint::new(name.into_name()?, array))
}

enum ParsedName {
    Builtin(ClientBuiltin),
    Named(Vec<String>),
    Float(Option<String>),
}

impl ParsedName {
    const fn builtin(name: ClientBuiltin) -> Self {
        Self::Builtin(name)
    }
    fn generic(head: &str, tail: &[String]) -> Self {
        let mut parts = vec![head.to_owned()];
        parts.extend_from_slice(tail);
        Self::Named(parts)
    }
    fn float(precision: &Option<String>) -> Self {
        Self::Float(precision.clone())
    }
    fn into_name(self) -> Result<ClientTypeName, AnalysisError> {
        Ok(match self {
            Self::Builtin(name) => ClientTypeName::Builtin(name),
            Self::Named(parts) => ClientTypeName::Named(match parts.as_slice() {
                [name] => ClientTypePath::Local(name.clone()),
                [schema, name] => ClientTypePath::Schema(schema.clone(), name.clone()),
                [catalog, schema, name] => {
                    ClientTypePath::Catalog(catalog.clone(), schema.clone(), name.clone())
                }
                _ => return Err(invalid("a type name has at most three components")),
            }),
            Self::Float(precision) => {
                ClientTypeName::Float(precision.as_deref().map(integer_value).transpose()?)
            }
        })
    }
}

fn simple_name<'a>() -> impl Parser<&'a [TypeToken], Output = ParsedName> {
    choice((
        attempt(numeric_name()),
        attempt(character_name()),
        attempt(datetime_name()),
        attempt(
            TypeToken::keyword("bit")
                .with(optional(TypeToken::keyword("varying")))
                .map(|varying| {
                    ParsedName::builtin(if varying.is_some() {
                        ClientBuiltin::BitVarying
                    } else {
                        ClientBuiltin::Bit
                    })
                }),
        ),
        attempt(
            TypeToken::keyword("interval").map(|()| ParsedName::builtin(ClientBuiltin::Interval)),
        ),
        attempt(TypeToken::keyword("json").map(|()| ParsedName::builtin(ClientBuiltin::Json))),
        (
            TypeToken::name(true),
            many::<Vec<_>, _, _>(TypeToken::punctuation(".").with(TypeToken::name(false))),
        )
            .map(|(head, tail)| ParsedName::generic(&head, &tail)),
    ))
}

fn numeric_name<'a>() -> impl Parser<&'a [TypeToken], Output = ParsedName> {
    choice((
        TypeToken::keyword("int")
            .or(TypeToken::keyword("integer"))
            .map(|()| ParsedName::builtin(ClientBuiltin::Integer)),
        TypeToken::keyword("smallint").map(|()| ParsedName::builtin(ClientBuiltin::Smallint)),
        TypeToken::keyword("bigint").map(|()| ParsedName::builtin(ClientBuiltin::Bigint)),
        TypeToken::keyword("real").map(|()| ParsedName::builtin(ClientBuiltin::Real)),
        TypeToken::keyword("boolean").map(|()| ParsedName::builtin(ClientBuiltin::Boolean)),
        TypeToken::keyword("decimal")
            .or(TypeToken::keyword("dec"))
            .or(TypeToken::keyword("numeric"))
            .map(|()| ParsedName::builtin(ClientBuiltin::Numeric)),
        (
            TypeToken::keyword("double"),
            TypeToken::keyword("precision"),
        )
            .map(|_| ParsedName::builtin(ClientBuiltin::DoublePrecision)),
        TypeToken::keyword("float")
            .with(optional(
                TypeToken::punctuation("(")
                    .with(TypeToken::integer())
                    .skip(TypeToken::punctuation(")")),
            ))
            .map(|precision| ParsedName::float(&precision)),
    ))
}

fn character_name<'a>() -> impl Parser<&'a [TypeToken], Output = ParsedName> {
    let character = choice((
        TypeToken::keyword("character"),
        TypeToken::keyword("char"),
        TypeToken::keyword("nchar"),
        TypeToken::keyword("national")
            .with(TypeToken::keyword("character").or(TypeToken::keyword("char"))),
    ));
    choice((
        TypeToken::keyword("varchar").map(|()| ParsedName::builtin(ClientBuiltin::Varchar)),
        character
            .with(optional(TypeToken::keyword("varying")))
            .map(|varying| {
                ParsedName::builtin(if varying.is_some() {
                    ClientBuiltin::Varchar
                } else {
                    ClientBuiltin::Character
                })
            }),
    ))
}

fn datetime_name<'a>() -> impl Parser<&'a [TypeToken], Output = ParsedName> {
    (
        TypeToken::keyword("timestamp")
            .map(|()| false)
            .or(TypeToken::keyword("time").map(|()| true)),
        optional((
            TypeToken::keyword("with")
                .map(|()| true)
                .or(TypeToken::keyword("without").map(|()| false)),
            TypeToken::keyword("time"),
            TypeToken::keyword("zone"),
        )),
    )
        .map(|(time, timezone)| {
            let with_zone = timezone.is_some_and(|(with_zone, (), ())| with_zone);
            ParsedName::builtin(match (time, with_zone) {
                (true, true) => ClientBuiltin::TimeWithZone,
                (true, false) => ClientBuiltin::Time,
                (false, true) => ClientBuiltin::TimestampWithZone,
                (false, false) => ClientBuiltin::Timestamp,
            })
        })
}

fn array_suffix<'a>() -> impl Parser<&'a [TypeToken], Output = bool> {
    choice((
        TypeToken::keyword("array")
            .with(optional(
                TypeToken::punctuation("[")
                    .with(TypeToken::integer())
                    .skip(TypeToken::punctuation("]")),
            ))
            .map(|_| true),
        many::<Vec<_>, _, _>(
            TypeToken::punctuation("[")
                .with(optional(TypeToken::integer()))
                .skip(TypeToken::punctuation("]")),
        )
        .map(|bounds| !bounds.is_empty()),
    ))
}

fn integer_value(text: &str) -> Result<u32, AnalysisError> {
    // PostgreSQL 16+ integer tokens include radix prefixes and digit separators.
    let text = text.replace('_', "").to_ascii_lowercase();
    let (digits, radix) = if let Some(digits) = text.strip_prefix("0x") {
        (digits, 16)
    } else if let Some(digits) = text.strip_prefix("0o") {
        (digits, 8)
    } else if let Some(digits) = text.strip_prefix("0b") {
        (digits, 2)
    } else {
        (text.as_str(), 10)
    };
    u32::from_str_radix(digits, radix).map_err(|_| invalid("invalid integer in type name"))
}
