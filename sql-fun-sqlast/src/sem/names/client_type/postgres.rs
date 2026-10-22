use super::token::{TypeToken, TypeTokens};
use super::{invalid, typmod};
use crate::sem::{AnalysisError, FullName, TypeReference};
use ::combine::{Parser, attempt, choice, many, optional};

pub(super) fn parse(tokens: &TypeTokens) -> Result<TypeReference, AnalysisError> {
    if tokens.interval_modifier() {
        return Err(typmod());
    }
    let ((name, array), remaining) = (simple_name(), array_suffix())
        .parse(tokens.as_slice())
        .map_err(|error| invalid(&format!("invalid PostgreSQL type syntax: {error:?}")))?;
    if let Some(token) = remaining.first() {
        return Err(if token.is_symbol("(") {
            typmod()
        } else {
            invalid("unexpected tokens after PostgreSQL type name")
        });
    }
    Ok(TypeReference::concrete_type_ref(&name.full_name()?, array))
}

struct ParsedName {
    parts: Vec<String>,
    system: bool,
    float_precision: Option<String>,
}

impl ParsedName {
    fn builtin(name: &str) -> Self {
        Self {
            parts: vec![name.to_owned()],
            system: true,
            float_precision: None,
        }
    }

    fn generic(head: &str, tail: &[String]) -> Self {
        let mut parts = vec![head.to_owned()];
        parts.extend_from_slice(tail);
        Self {
            parts,
            system: false,
            float_precision: None,
        }
    }

    fn float(precision: &Option<String>) -> Self {
        Self {
            float_precision: precision.clone(),
            ..Self::builtin("float8")
        }
    }

    fn full_name(&self) -> Result<FullName, AnalysisError> {
        if let Some(precision) = &self.float_precision {
            let precision = integer_value(precision)?;
            let name = match precision {
                1..=24 => "float4",
                25..=53 => "float8",
                _ => return Err(invalid("float precision must be between 1 and 53")),
            };
            return Ok(FullName::with_schema("pg_catalog", name));
        }
        match self.parts.as_slice() {
            [name] if self.system => Ok(FullName::with_schema("pg_catalog", name)),
            [name] => Ok(FullName::from_local_name(name)),
            [schema, name] => Ok(FullName::with_schema(schema, name)),
            [catalog, schema, name] => Ok(FullName::with_catalog(catalog, schema, name)),
            _ => Err(invalid("a type name has at most three components")),
        }
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
                    ParsedName::builtin(if varying.is_some() { "varbit" } else { "bit" })
                }),
        ),
        attempt(TypeToken::keyword("interval").map(|()| ParsedName::builtin("interval"))),
        attempt(TypeToken::keyword("json").map(|()| ParsedName::builtin("json"))),
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
            .map(|()| ParsedName::builtin("int4")),
        TypeToken::keyword("smallint").map(|()| ParsedName::builtin("int2")),
        TypeToken::keyword("bigint").map(|()| ParsedName::builtin("int8")),
        TypeToken::keyword("real").map(|()| ParsedName::builtin("float4")),
        TypeToken::keyword("boolean").map(|()| ParsedName::builtin("bool")),
        TypeToken::keyword("decimal")
            .or(TypeToken::keyword("dec"))
            .or(TypeToken::keyword("numeric"))
            .map(|()| ParsedName::builtin("numeric")),
        (
            TypeToken::keyword("double"),
            TypeToken::keyword("precision"),
        )
            .map(|_| ParsedName::builtin("float8")),
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
        TypeToken::keyword("varchar").map(|()| ParsedName::builtin("varchar")),
        character
            .with(optional(TypeToken::keyword("varying")))
            .map(|varying| {
                ParsedName::builtin(if varying.is_some() {
                    "varchar"
                } else {
                    "bpchar"
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
                (true, true) => "timetz",
                (true, false) => "time",
                (false, true) => "timestamptz",
                (false, false) => "timestamp",
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
