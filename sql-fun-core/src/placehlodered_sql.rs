use std::collections::HashSet;

use combine::{
    Parser, between, many, optional,
    parser::{
        char::{alpha_num, char, letter, spaces, string},
        range::recognize,
    },
    satisfy,
};
use indexmap::IndexMap;

#[derive(Debug, Hash, Eq, PartialEq, Clone, serde::Deserialize)]
#[serde(transparent)]
pub struct ParameterName(String);

impl<T> From<T> for ParameterName
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        Self(String::from(value.as_ref()))
    }
}

#[derive(Debug, Clone)]
pub struct PlaceholderTypeHint(String);

impl PlaceholderTypeHint {
    pub fn hint_value(&self) -> &str {
        self.0.as_str()
    }
}

fn ident<'a>() -> impl combine::Parser<&'a str, Output = String> {
    (letter().or(char('_')), many(alpha_num().or(char('_')))).map(|(head, tail): (char, String)| {
        let mut s = String::with_capacity(1 + tail.len());
        s.push(head);
        s.push_str(&tail);
        s
    })
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub struct FragmentName(String);

impl FragmentName {
    /// get perser accepts `#[ fragment_name ]`
    pub fn parser<'a>() -> impl combine::Parser<&'a str, Output = Self> {
        let ident = ident().map(FragmentName);

        char('#').with(between(
            char('[').skip(spaces()),
            spaces().with(char(']')),
            ident,
        ))
    }
}

#[derive(Debug)]
pub struct NamedParameterPlaceholder {
    name: ParameterName,
    type_hint: Option<PlaceholderTypeHint>,
}

impl NamedParameterPlaceholder {
    /// get parser accepts `${ name:type_hint }` or `${name}`
    pub fn parser<'a>() -> impl combine::Parser<&'a str, Output = Self> {
        let name_with_type_opt = (ident(), optional(ident())).map(|(name, type_hint)| Self {
            name: ParameterName(name),
            type_hint: type_hint.map(PlaceholderTypeHint),
        });
        char('$').with(between(
            char('{').skip(spaces()),
            spaces().with(char('}')),
            name_with_type_opt,
        ))
    }

    /// get a parameter name
    pub fn name(&self) -> &ParameterName {
        &self.name
    }

    /// get a type hint
    pub fn type_hint(&self) -> Option<&PlaceholderTypeHint> {
        self.type_hint.as_ref()
    }
}

#[derive(Debug)]
pub struct SqlLiteralText(String);

impl SqlLiteralText {
    pub fn text(&self) -> &str {
        self.0.as_str()
    }
}

impl SqlLiteralText {
    /// get parser accepts SQL Literal text
    pub fn parser<'a>() -> impl combine::Parser<&'a str, Output = Self> {
        let escaped_char = string(r"''").map(|_| '\'');

        recognize(between(
            char('\''),
            char('\''),
            many::<Vec<_>, _, _>(escaped_char.or(satisfy(|c| c != '\''))).map(|_| ()),
        ))
        .map(|s| Self(String::from(s)))
    }
}

#[derive(Debug)]
pub struct RawSqlText(String);

impl RawSqlText {
    pub fn text(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug)]
pub enum PlaceholderedSqlToken {
    ParameterPlaceholder(NamedParameterPlaceholder),
    FragmentPlaceholder(FragmentName),
    LiteralText(SqlLiteralText),
    RawSqlText(RawSqlText),
}

impl PlaceholderedSqlToken {
    pub fn try_take_special_item(source: &str) -> Result<(Self, &str), &str> {
        if let Ok((item, rest)) = NamedParameterPlaceholder::parser().parse(source) {
            return Ok((Self::ParameterPlaceholder(item), rest));
        }

        if let Ok((item, rest)) = FragmentName::parser().parse(source) {
            return Ok((Self::FragmentPlaceholder(item), rest));
        }

        if let Ok((item, rest)) = SqlLiteralText::parser().parse(source) {
            return Ok((Self::LiteralText(item), rest));
        }
        Err(source)
    }

    pub fn scan(input: &str) -> Vec<Self> {
        fn take_push_char<'a>(input: &'a str, buffer: &mut String) -> &'a str {
            assert!(!input.is_empty());

            let mut chars = input.char_indices();
            let (_, first_char) = chars.next().unwrap(); // safe to unwrap, asserted
            buffer.push(first_char);
            let Some((rest_start, _)) = chars.next() else {
                return &input[input.len()..];
            };
            &input[rest_start..]
        }

        let mut results = Vec::new();
        let mut rest = input;
        let mut buffer = String::new();
        while !rest.is_empty() {
            match Self::try_take_special_item(rest) {
                Ok((item, new_rest)) => {
                    if !buffer.is_empty() {
                        buffer.shrink_to_fit();
                        results.push(Self::RawSqlText(RawSqlText(buffer)));
                        buffer = String::new();
                    }
                    results.push(item);
                    rest = new_rest;
                }
                Err(_) => {
                    rest = take_push_char(rest, &mut buffer);
                }
            }
        }
        if !buffer.is_empty() {
            results.push(Self::RawSqlText(RawSqlText(buffer.clone())));
        }
        results
    }
}

#[derive(Debug)]
/// SQL text contains `sql-fun` named parameter syntax and fragment embedding marker
pub struct PlaceholderedSql {
    tokens: Vec<PlaceholderedSqlToken>,
    parameters: IndexMap<ParameterName, Vec<PlaceholderTypeHint>>,
    fragments: HashSet<FragmentName>,
}

impl PlaceholderedSql {
    /// build from query text string
    #[must_use]
    pub fn from_query(query: &str) -> Self {
        let tokens = PlaceholderedSqlToken::scan(query);
        let mut parameters: IndexMap<ParameterName, Vec<PlaceholderTypeHint>> = IndexMap::new();
        let mut fragments: HashSet<FragmentName> = HashSet::new();

        for token in &tokens {
            match token {
                PlaceholderedSqlToken::ParameterPlaceholder(named_parameter_placeholder) => {
                    let parameter_name = named_parameter_placeholder.name();
                    if let Some(parameter) = parameters.get_mut(parameter_name) {
                        if let Some(hint) = named_parameter_placeholder.type_hint() {
                            parameter.push(hint.clone());
                        }
                    } else {
                        let mut hints = Vec::new();
                        if let Some(hint) = named_parameter_placeholder.type_hint() {
                            hints.push(hint.clone());
                        }
                        parameters.insert(parameter_name.clone(), hints);
                    }
                }
                PlaceholderedSqlToken::FragmentPlaceholder(fragment_name) => {
                    fragments.insert(fragment_name.clone());
                }
                _ => {}
            }
        }

        Self {
            tokens,
            parameters,
            fragments,
        }
    }

    /// fragment embedding definition is empty
    #[must_use]
    pub fn fragments_is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    /// get parameter placeholders
    pub fn parameters(&self) -> impl Iterator<Item = (&ParameterName, &Vec<PlaceholderTypeHint>)> {
        self.parameters.iter()
    }

    /// get tokens in sql
    pub fn tokens(&self) -> impl Iterator<Item = &PlaceholderedSqlToken> {
        self.tokens.iter()
    }
}
