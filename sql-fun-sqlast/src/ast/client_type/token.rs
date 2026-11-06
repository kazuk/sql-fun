use crate::{ast::client_type::invalid, error::AnalysisError};
use ::combine::{Parser, satisfy};

/// Local lexical representation; existing dialect scanners own keyword policy.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TypeToken {
    text: String,
    keyword: bool,
    first_name: bool,
    integer: bool,
}

impl TypeToken {
    pub(super) fn symbol(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            keyword: false,
            first_name: false,
            integer: false,
        }
    }

    pub(super) fn identifier(text: &str) -> Self {
        Self {
            first_name: true,
            ..Self::symbol(text)
        }
    }

    pub(super) fn is_keyword(&self, word: &str) -> bool {
        self.keyword && self.text == word
    }

    pub(super) fn is_symbol(&self, symbol: &str) -> bool {
        !self.first_name && !self.keyword && self.text == symbol
    }

    pub(super) fn name<'a>(first: bool) -> impl Parser<&'a [Self], Output = String> {
        satisfy(move |token: Self| {
            if first {
                token.first_name
            } else {
                token.first_name || token.keyword
            }
        })
        .map(|token: Self| token.text)
    }

    pub(super) fn keyword<'a>(word: &'static str) -> impl Parser<&'a [Self], Output = ()> {
        satisfy(move |token: Self| token.is_keyword(word)).map(|_| ())
    }

    pub(super) fn punctuation<'a>(symbol: &'static str) -> impl Parser<&'a [Self], Output = ()> {
        satisfy(move |token: Self| token.is_symbol(symbol)).map(|_| ())
    }

    pub(super) fn integer<'a>() -> impl Parser<&'a [Self], Output = String> {
        satisfy(|token: Self| token.integer).map(|token: Self| token.text)
    }
}

pub(super) struct TypeTokens(Vec<TypeToken>);

impl TypeTokens {
    pub(super) fn as_slice(&self) -> &[TypeToken] {
        &self.0
    }

    pub(super) fn postgres(input: &str) -> Result<Self, AnalysisError> {
        use ::pg_query::protobuf::{KeywordKind, Token};
        let scanned = ::pg_query::scan(input).map_err(|error| invalid(&error.to_string()))?;
        let mut tokens = Vec::new();
        let mut source = scanned.tokens.into_iter().peekable();
        while let Some(token) = source.next() {
            let kind = token.token();
            if matches!(kind, Token::SqlComment | Token::CComment) {
                continue;
            }
            let raw = &input[token.start as usize..token.end as usize];
            let result = if kind == Token::Uident {
                let mut escape = '\\';
                // UESCAPE belongs to this identifier, including intervening comments.
                while source
                    .peek()
                    .is_some_and(|next| matches!(next.token(), Token::SqlComment | Token::CComment))
                {
                    source.next();
                }
                if source
                    .peek()
                    .is_some_and(|next| next.token() == Token::Uescape)
                {
                    source.next();
                    while source.peek().is_some_and(|next| {
                        matches!(next.token(), Token::SqlComment | Token::CComment)
                    }) {
                        source.next();
                    }
                    let next = source
                        .next()
                        .ok_or_else(|| invalid("missing UESCAPE character"))?;
                    if next.token() != Token::Sconst {
                        return Err(invalid("invalid UESCAPE character"));
                    }
                    let end = continued_literal_end(input, next.end as usize, &mut source);
                    let decoded = escape_literal(&input[next.start as usize..end])?;
                    let mut chars = decoded.chars();
                    escape = chars.next().ok_or_else(|| invalid("empty UESCAPE"))?;
                    if chars.next().is_some()
                        || !escape.is_ascii()
                        || escape.is_ascii_hexdigit()
                        || escape.is_ascii_whitespace()
                        || matches!(escape, '+' | '\'' | '"')
                    {
                        return Err(invalid("invalid UESCAPE character"));
                    }
                }
                TypeToken::identifier(&postgres_identifier(&unicode_identifier(raw, escape)?)?)
            } else if kind == Token::Ident {
                let name = if raw.starts_with('"') {
                    raw[1..raw.len() - 1].replace("\"\"", "\"")
                } else {
                    raw.to_ascii_lowercase()
                };
                TypeToken::identifier(&postgres_identifier(&name)?)
            } else if token.keyword_kind() != KeywordKind::NoKeyword {
                let mut result = TypeToken::identifier(&raw.to_ascii_lowercase());
                result.keyword = true;
                result.first_name = matches!(
                    token.keyword_kind(),
                    KeywordKind::UnreservedKeyword | KeywordKind::TypeFuncNameKeyword
                );
                result
            } else {
                let mut result = TypeToken::symbol(raw);
                result.integer = kind == Token::Iconst;
                result
            };
            tokens.push(result);
        }
        Ok(Self(tokens))
    }

    #[cfg(feature = "sqlite")]
    pub(super) fn sqlite(input: &str) -> Result<Self, AnalysisError> {
        use ::turso_parser::{lexer::Lexer, token::TokenType};
        let mut lexer = Lexer::new(input.as_bytes());
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            let token = token.map_err(|error| invalid(&error.to_string()))?;
            if token.token_type == TokenType::TK_NONE {
                continue;
            }
            let raw =
                std::str::from_utf8(token.value).map_err(|error| invalid(&error.to_string()))?;
            if token.token_type == TokenType::TK_LBRACKET
                && (tokens.is_empty() || !lexer.remaining().starts_with(b"]"))
            {
                let remaining = std::str::from_utf8(lexer.remaining())
                    .map_err(|error| invalid(&error.to_string()))?;
                let (name, tail) = remaining
                    .split_once(']')
                    .ok_or_else(|| invalid("unterminated bracket-quoted identifier"))?;
                tokens.push(TypeToken::identifier(name));
                lexer = Lexer::new(tail.as_bytes());
            } else if matches!(
                token.token_type.fallback_id_if_ok(),
                TokenType::TK_ID | TokenType::TK_STRING
            ) {
                let name = ::turso_parser::ast::Name::from_string(raw);
                tokens.push(TypeToken::identifier(name.as_str()));
            } else {
                tokens.push(TypeToken::symbol(raw));
            }
        }
        Ok(Self(tokens))
    }

    pub(super) fn interval_modifier(&self) -> bool {
        self.0
            .first()
            .is_some_and(|token| token.is_keyword("interval"))
            && self.0.get(1).is_some_and(|token| {
                ["year", "month", "day", "hour", "minute", "second"]
                    .into_iter()
                    .any(|word| token.is_keyword(word))
            })
    }
}

fn postgres_identifier(name: &str) -> Result<String, AnalysisError> {
    if name.is_empty() || name.contains('\0') {
        return Err(invalid("empty or NUL identifier"));
    }
    // PostgreSQL NAMEDATALEN includes the terminating NUL; don't split UTF-8.
    let mut end = name.len().min(63);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    Ok(name[..end].to_owned())
}

fn unicode_identifier(raw: &str, escape: char) -> Result<String, AnalysisError> {
    let body = raw
        .get(3..raw.len() - 1)
        .ok_or_else(|| invalid("Unicode identifier"))?;
    let body = body.replace("\"\"", "\"");
    let mut chars = body.chars();
    let mut output = String::new();
    while let Some(character) = chars.next() {
        if character != escape {
            output.push(character);
            continue;
        }
        let first = chars
            .next()
            .ok_or_else(|| invalid("incomplete Unicode escape"))?;
        if first == escape {
            output.push(escape);
            continue;
        }
        let mut code = unicode_code(first, &mut chars)?;
        if (0xd800..=0xdbff).contains(&code) {
            if chars.next() != Some(escape) {
                return Err(invalid("unpaired Unicode surrogate"));
            }
            let first = chars
                .next()
                .ok_or_else(|| invalid("incomplete Unicode surrogate"))?;
            let low = unicode_code(first, &mut chars)?;
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(invalid("invalid Unicode surrogate pair"));
            }
            code = 0x10000 + ((code - 0xd800) << 10) + low - 0xdc00;
        }
        let decoded = char::from_u32(code)
            .filter(|value| *value != '\0')
            .ok_or_else(|| invalid("invalid Unicode code point"))?;
        output.push(decoded);
    }
    Ok(output)
}

fn unicode_code(first: char, chars: &mut std::str::Chars<'_>) -> Result<u32, AnalysisError> {
    let (mut value, digits) = if first == '+' {
        (0, 6)
    } else {
        (
            first
                .to_digit(16)
                .ok_or_else(|| invalid("invalid Unicode escape"))?,
            3,
        )
    };
    for _ in 0..digits {
        let digit = next_hex_digit(chars)?;
        value = value * 16 + digit;
    }
    Ok(value)
}

fn escape_literal(raw: &str) -> Result<String, AnalysisError> {
    let escaped = raw.starts_with(['e', 'E']);
    let raw = if escaped { &raw[1..] } else { raw };
    let mut chars = raw.chars().peekable();
    if chars.next() != Some('\'') {
        return Err(invalid("invalid UESCAPE string"));
    }
    let mut output = Vec::new();
    while let Some(character) = chars.next() {
        if character == '\'' {
            if chars.peek() == Some(&'\'') {
                chars.next();
                output.push(b'\'');
                continue;
            }
            // The SQL scanner permits newline-separated literal concatenation.
            let tail = chars.clone().collect::<String>();
            if tail.trim().is_empty() {
                return String::from_utf8(output).map_err(|error| invalid(&error.to_string()));
            }
            skip_literal_continuation_trivia(&mut chars);
            if chars.next() != Some('\'') {
                return Err(invalid("invalid UESCAPE string continuation"));
            }
        } else if escaped && character == '\\' {
            let next = chars
                .next()
                .ok_or_else(|| invalid("incomplete escape string"))?;
            let byte = match next {
                'b' => 8,
                'f' => 12,
                'n' => b'\n',
                'r' => b'\r',
                't' => b'\t',
                '0'..='7' => {
                    let mut value = next.to_digit(8).unwrap_or(0);
                    for _ in 0..2 {
                        let Some(character) = chars.peek() else {
                            break;
                        };
                        let Some(digit) = character.to_digit(8) else {
                            break;
                        };
                        chars.next();
                        value = value * 8 + digit;
                    }
                    (value & 255) as u8
                }
                'x' => {
                    let mut value = 0;
                    let mut count = 0;
                    for _ in 0..2 {
                        let Some(character) = chars.peek() else {
                            break;
                        };
                        let Some(digit) = character.to_digit(16) else {
                            break;
                        };
                        chars.next();
                        value = value * 16 + digit;
                        count += 1;
                    }
                    if count == 0 { b'x' } else { value as u8 }
                }
                'u' | 'U' => {
                    let mut value = 0;
                    for _ in 0..if next == 'u' { 4 } else { 8 } {
                        let digit = next_hex_digit(&mut chars)?;
                        value = value * 16 + digit;
                    }
                    let value = char::from_u32(value)
                        .ok_or_else(|| invalid("invalid Unicode escape string"))?;
                    output.extend_from_slice(value.encode_utf8(&mut [0; 4]).as_bytes());
                    continue;
                }
                value => {
                    output.extend_from_slice(value.encode_utf8(&mut [0; 4]).as_bytes());
                    continue;
                }
            };
            output.push(byte);
        } else {
            output.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    Err(invalid("unterminated UESCAPE string"))
}

fn next_hex_digit(chars: &mut impl Iterator<Item = char>) -> Result<u32, AnalysisError> {
    let Some(character) = chars.next() else {
        return Err(invalid("incomplete Unicode escape"));
    };
    character
        .to_digit(16)
        .ok_or_else(|| invalid("invalid Unicode escape"))
}

fn skip_literal_continuation_trivia(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    loop {
        while chars
            .peek()
            .is_some_and(|value| value.is_ascii_whitespace())
        {
            chars.next();
        }
        if chars.peek() != Some(&'-') || chars.clone().nth(1) != Some('-') {
            break;
        }
        for character in chars.by_ref() {
            if matches!(character, '\n' | '\r') {
                break;
            }
        }
    }
}

fn continued_literal_end(
    input: &str,
    mut end: usize,
    source: &mut std::iter::Peekable<std::vec::IntoIter<::pg_query::protobuf::ScanToken>>,
) -> usize {
    use ::pg_query::protobuf::Token;
    // Unlike the statement lexer, pg_query::scan emits line comments as tokens.
    // Rejoin only the string continuations accepted by PostgreSQL's quote rule.
    loop {
        let mut continuation = source.clone();
        while continuation
            .peek()
            .is_some_and(|token| token.token() == Token::SqlComment)
        {
            continuation.next();
        }
        let Some(next) = continuation.peek().cloned() else {
            break;
        };
        if next.token() != Token::Sconst
            || !input[next.start as usize..].starts_with('\'')
            || !input[end..next.start as usize].contains(['\n', '\r'])
        {
            break;
        }
        end = next.end as usize;
        continuation.next();
        *source = continuation;
    }
    end
}
