use std::collections::HashSet;

use combine::{
    Parser, attempt, between, many, optional,
    parser::char::{alpha_num, char, letter, spaces, string},
    satisfy,
};
use indexmap::IndexMap;
use toml_parser::decoder::StringBuilder;

#[derive(Debug, Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
/// A named sql-fun query parameter.
pub struct ParameterName(String);

impl From<&str> for ParameterName {
    fn from(value: &str) -> Self {
        Self(String::from(value))
    }
}

impl From<String> for ParameterName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl ParameterName {
    /// get the parameter name text
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn encode(&self, buffer: &mut String) {
        buffer.push_str(&self.0);
    }
}

impl AsRef<str> for ParameterName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct PlaceholderTypeHint(String);

impl PlaceholderTypeHint {
    pub const fn hint_value(&self) -> &str {
        self.0.as_str()
    }

    fn encode(&self, buffer: &mut String) {
        buffer.push_str(&self.0);
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

/// fragment embedding placeholder name
#[derive(Debug, Hash, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FragmentName(String);

impl FragmentName {
    /// create instance
    pub fn new<S>(name: S) -> Self
    where
        S: AsRef<str>,
    {
        Self(String::from(name.as_ref()))
    }

    /// get perser accepts `#[ fragment_name ]`
    pub fn parser<'a>() -> impl combine::Parser<&'a str, Output = Self> {
        let ident = ident().map(FragmentName);

        char('#').with(between(
            char('[').skip(spaces()),
            spaces().with(char(']')),
            ident,
        ))
    }

    /// get `&str` value
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn encode(&self, buffer: &mut String) {
        let () = buffer.push_str("#[");
        let () = buffer.push_str(&self.0);
        let _ = buffer.push_char(']');
    }
}

impl std::fmt::Display for FragmentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NamedParameterPlaceholder {
    name: ParameterName,
    type_hint: Option<PlaceholderTypeHint>,
}

impl NamedParameterPlaceholder {
    /// get parser accepts `${ name:type_hint }` or `${name}`
    pub fn parser<'a>() -> impl combine::Parser<&'a str, Output = Self> {
        let name_with_type_opt =
            (ident(), optional(char(':').with(ident()))).map(|(name, type_hint)| Self {
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
    pub const fn name(&self) -> &ParameterName {
        &self.name
    }

    /// get a type hint
    pub const fn type_hint(&self) -> Option<&PlaceholderTypeHint> {
        self.type_hint.as_ref()
    }

    fn encode(&self, buffer: &mut String) {
        buffer.push_str("${");
        self.name.encode(buffer);
        if let Some(type_hint) = &self.type_hint {
            buffer.push(':');
            type_hint.encode(buffer);
        }
        buffer.push('}');
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SqlLiteralText(String);

impl SqlLiteralText {
    /// get literal contents without SQL quote delimiters
    pub const fn text(&self) -> &str {
        self.0.as_str()
    }

    /// encode as SQL literal text
    #[must_use]
    pub fn sql(&self) -> String {
        let mut buffer = String::new();
        self.encode(&mut buffer);
        buffer
    }
}

impl SqlLiteralText {
    /// get parser accepts SQL Literal text
    pub fn parser<'a>() -> impl combine::Parser<&'a str, Output = Self> {
        let escaped_char = string(r"''").map(|_| '\'');

        between(
            char('\''),
            char('\''),
            many::<Vec<_>, _, _>(attempt(escaped_char).or(satisfy(|c| c != '\''))),
        )
        .map(|text| Self(text.into_iter().collect()))
    }

    fn encode(&self, buffer: &mut String) {
        let _ = buffer.push_char('\'');
        let () = buffer.push_str(&self.0.replace('\'', r"''"));
        let _ = buffer.push_char('\'');
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RawSqlText(String);

impl RawSqlText {
    pub const fn text(&self) -> &str {
        self.0.as_str()
    }

    fn encode(&self, buffer: &mut String) {
        buffer.push_str(&self.0);
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
/// A token in SQL containing sql-fun placeholders.
pub enum PlaceholderedSqlToken {
    /// A named parameter placeholder such as `${user_id}`.
    ParameterPlaceholder(NamedParameterPlaceholder),
    /// A fragment placeholder such as `#[where_clause]`.
    FragmentPlaceholder(FragmentName),
    /// A SQL string literal.
    LiteralText(SqlLiteralText),
    /// SQL text that is neither a placeholder nor a string literal.
    RawSqlText(RawSqlText),
}

impl PlaceholderedSqlToken {
    /// Parse one special token from the start of `source`.
    ///
    /// # Errors
    ///
    /// Returns the original source when it does not begin with a special token.
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

    fn take_push_char<'a>(input: &'a str, buffer: &mut String) -> &'a str {
        assert_ne!(input, "");

        let mut chars = input.char_indices();
        let (_, first_char) = chars.next().unwrap(); // safe to unwrap, asserted
        buffer.push(first_char);
        let Some((rest_start, _)) = chars.next() else {
            return &input[input.len()..];
        };
        &input[rest_start..]
    }

    /// Consume a host-SQL lexical region that must remain opaque to sql-fun.
    ///
    /// sql-fun special tokens are parsed by the `combine` parsers in
    /// [`Self::try_take_special_item`]. Host-SQL regions are recognized first
    /// because nested block comments and dollar-quoted strings require state
    /// or a delimiter selected by the input, neither of which is a fixed
    /// sql-fun token grammar. Keeping this boundary here ensures later
    /// additions choose `combine` for sql-fun syntax and this scanner only for
    /// opaque host-SQL lexical regions.
    fn take_host_sql_region<'a>(input: &'a str, buffer: &mut String) -> Option<&'a str> {
        Self::take_comment(input, buffer)
            .or_else(|| Self::take_dollar_quoted(input, buffer))
            .or_else(|| Self::take_quoted_identifier(input, buffer))
    }

    fn take_comment<'a>(input: &'a str, buffer: &mut String) -> Option<&'a str> {
        if let Some(comment) = input.strip_prefix("--") {
            let len = comment
                .find('\n')
                .map_or(comment.len(), |offset| offset + 1);
            buffer.push_str(&input[..2 + len]);
            return Some(&input[2 + len..]);
        }
        input.strip_prefix("/*")?;
        let mut depth = 1;
        let mut offset = 2;
        while offset < input.len() {
            if input[offset..].starts_with("/*") {
                depth += 1;
                offset += 2;
                continue;
            }
            if input[offset..].starts_with("*/") {
                depth -= 1;
                offset += 2;
                if depth == 0 {
                    buffer.push_str(&input[..offset]);
                    return Some(&input[offset..]);
                }
                continue;
            }
            let character = input[offset..]
                .chars()
                .next()
                .expect("offset is within input");
            offset += character.len_utf8();
        }
        buffer.push_str(input);
        Some("")
    }

    fn take_quoted_identifier<'a>(input: &'a str, buffer: &mut String) -> Option<&'a str> {
        input.strip_prefix('"')?;
        let mut offset = 1;
        while offset < input.len() {
            if input[offset..].starts_with("\"\"") {
                offset += 2;
                continue;
            }
            if input[offset..].starts_with('"') {
                offset += 1;
                buffer.push_str(&input[..offset]);
                return Some(&input[offset..]);
            }
            let character = input[offset..]
                .chars()
                .next()
                .expect("offset is within input");
            offset += character.len_utf8();
        }
        buffer.push_str(input);
        Some("")
    }

    fn take_dollar_quoted<'a>(input: &'a str, buffer: &mut String) -> Option<&'a str> {
        input.strip_prefix('$')?;
        let mut offset = 1;
        if input[offset..].starts_with('$') {
            offset += 1;
        } else {
            let first = input[offset..].chars().next()?;
            if !(first.is_ascii_alphabetic() || first == '_') {
                return None;
            }
            offset += first.len_utf8();
            while let Some(character) = input[offset..].chars().next() {
                if !(character.is_ascii_alphanumeric() || character == '_') {
                    break;
                }
                offset += character.len_utf8();
            }
            if !input[offset..].starts_with('$') {
                return None;
            }
            offset += 1;
        }
        let delimiter = &input[..offset];
        let content = &input[offset..];
        let len = content
            .find(delimiter)
            .map_or(input.len(), |end| offset + end + delimiter.len());
        buffer.push_str(&input[..len]);
        Some(&input[len..])
    }

    /// Split SQL into placeholder-aware tokens.
    ///
    /// SQL fragments with delimiters that nest or are selected by the input
    /// are preserved as raw text before the `combine`-based sql-fun token
    /// parsers are applied. This keeps placeholders inside host-SQL comments,
    /// dollar-quoted strings, and quoted identifiers from being substituted.
    #[must_use]
    pub fn scan(input: &str) -> Vec<Self> {
        let mut results = Vec::new();
        let mut rest = input;
        let mut buffer = String::new();
        while !rest.is_empty() {
            if let Some(new_rest) = Self::take_host_sql_region(rest, &mut buffer) {
                rest = new_rest;
                continue;
            }
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
                    rest = Self::take_push_char(rest, &mut buffer);
                }
            }
        }
        if !buffer.is_empty() {
            results.push(Self::RawSqlText(RawSqlText(buffer.clone())));
        }
        results
    }

    /// Return the fragment name when this is a fragment placeholder.
    #[must_use]
    pub const fn as_fragment_placeholder(&self) -> Option<&FragmentName> {
        match self {
            Self::FragmentPlaceholder(v) => Some(v),
            _ => None,
        }
    }

    fn is_fragment_place_holder(&self, fragment_name: &FragmentName) -> bool {
        match self {
            Self::FragmentPlaceholder(v) => v == fragment_name,
            _ => false,
        }
    }

    fn encode(&self, buffer: &mut String) {
        match self {
            Self::ParameterPlaceholder(named_parameter_placeholder) => {
                named_parameter_placeholder.encode(buffer);
            }
            Self::FragmentPlaceholder(fragment_name) => fragment_name.encode(buffer),
            Self::LiteralText(sql_literal_text) => sql_literal_text.encode(buffer),
            Self::RawSqlText(raw_sql_text) => raw_sql_text.encode(buffer),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
/// SQL text contains `sql-fun` named parameter syntax and fragment embedding marker
pub struct PlaceholderedSql {
    tokens: Vec<PlaceholderedSqlToken>,
    parameters: IndexMap<ParameterName, Vec<PlaceholderTypeHint>>,
    fragments: HashSet<FragmentName>,
}

/// Placeholder processing errors
#[derive(thiserror::Error, Debug)]
pub enum PlaceholderedSqlError {
    /// fragment placeholder not found
    #[error("fragment placeholder {0} not found")]
    FragmentPlaceholderNotFound(FragmentName),
}

impl PlaceholderedSql {
    /// build from query text string
    #[must_use]
    pub fn from_query(query: &str) -> Self {
        let tokens = PlaceholderedSqlToken::scan(query);
        Self::from_tokens(tokens)
    }

    /// build from tokens
    #[must_use]
    pub fn from_tokens(tokens: Vec<PlaceholderedSqlToken>) -> Self {
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

    /// enumerate fragment by usage order
    pub fn fragment_names_by_usage_order(&self) -> impl Iterator<Item = &FragmentName> {
        self.tokens
            .iter()
            .filter_map(|t| t.as_fragment_placeholder())
    }

    /// get parameter placeholders
    pub fn parameters(&self) -> impl Iterator<Item = (&ParameterName, &Vec<PlaceholderTypeHint>)> {
        self.parameters.iter()
    }

    /// get tokens in sql
    pub fn tokens(&self) -> impl Iterator<Item = &PlaceholderedSqlToken> {
        self.tokens.iter()
    }

    /// embed fragment SQL text
    ///
    /// # Errors
    ///
    /// [`PlaceholderedSqlError`]
    pub fn embed_fragment(
        &self,
        fragment_name: &FragmentName,
        sql_fragment: &str,
    ) -> Result<Self, PlaceholderedSqlError> {
        let embedding = PlaceholderedSqlToken::scan(sql_fragment);
        let mut rebuilt_tokens: Vec<PlaceholderedSqlToken> = Vec::new();
        for item in self.tokens() {
            if item.is_fragment_place_holder(fragment_name) {
                for token in embedding.iter().cloned() {
                    rebuilt_tokens.push(token);
                }
            } else {
                rebuilt_tokens.push(item.clone());
            }
        }
        Ok(Self::from_tokens(rebuilt_tokens))
    }

    /// encode into string
    #[must_use]
    pub fn encode(&self) -> String {
        let mut buffer = String::new();
        for token in &self.tokens {
            token.encode(&mut buffer);
        }
        buffer
    }
}
