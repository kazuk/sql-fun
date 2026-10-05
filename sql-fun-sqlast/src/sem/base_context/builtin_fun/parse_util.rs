// See `./builtin-metadata-format.md` for the rationale and
// expected replacement of this v1 representation.
//
use combine::EasyParser;

pub fn parse_braced_list_tokens(input: &str) -> Result<Vec<String>, String> {
    use combine::{
        Parser,
        parser::{char::char, char::spaces, range::take_while1},
        sep_by,
    };

    let elem = take_while1(|c: char| c != ',' && c != '}' && !c.is_whitespace())
        .map(|s: &str| s.to_owned());
    let comma = spaces().with(char(',')).skip(spaces());
    let mut parser = char('{')
        .skip(spaces())
        .with(sep_by(elem, comma))
        .skip(spaces())
        .skip(char('}'));
    let (items, _rest) = parser.easy_parse(input).map_err(|e| e.to_string())?;

    Ok(items)
}

pub fn deserialize_braced_list<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let s: String = serde::de::Deserialize::deserialize(d)?;
    let items = parse_braced_list_tokens(&s).map_err(serde::de::Error::custom)?;
    let mut results = Vec::new();
    for item in items {
        results.push(T::from_str(&item).map_err(serde::de::Error::custom)?);
    }
    Ok(results)
}

pub fn deserialize_braced_list_opt<'de, D, T>(d: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let s: Option<String> = serde::de::Deserialize::deserialize(d)?;
    let Some(list) = s else {
        return Ok(None);
    };

    let items = parse_braced_list_tokens(&list).map_err(serde::de::Error::custom)?;
    let mut results = Vec::new();
    for item in items {
        results.push(T::from_str(&item).map_err(serde::de::Error::custom)?);
    }
    Ok(Some(results))
}
