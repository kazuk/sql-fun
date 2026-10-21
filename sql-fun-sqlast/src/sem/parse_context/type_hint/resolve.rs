//! Construct named type references directly, without a statement or expression AST.
use crate::ast::Dialect;
use crate::sem::{AnalysisError, FullName, ParseContext, TypeReference};

pub(super) fn resolve_parameter_hint(
    hint: &str,
    context: &impl ParseContext,
) -> Result<Option<TypeReference>, AnalysisError> {
    let reference = if context.dialect() == Dialect::postgres() {
        postgres_reference(hint)?
    } else {
        #[cfg(feature = "sqlite")]
        {
            sqlite_reference(hint)?
        }
        #[cfg(not(feature = "sqlite"))]
        {
            return Err(invalid_hint());
        }
    };
    let canonical = context.canonicalize_type_ref(&reference).ok_or_else(|| {
        AnalysisError::unexpected_input_error(&format!("SQL type {reference} not found"))
    })?;
    // PostgreSQL treats an explicitly declared unknown parameter as unspecified.
    if context.dialect() == Dialect::postgres()
        && canonical == TypeReference::from_master_type_name("unknown")
    {
        Ok(None)
    } else {
        Ok(Some(canonical))
    }
}

fn postgres_reference(hint: &str) -> Result<TypeReference, AnalysisError> {
    use ::pg_query::protobuf::Token;
    let scanned = ::pg_query::scan(hint)
        .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
    let words: Vec<_> = scanned
        .tokens
        .iter()
        .filter(|token| !matches!(token.token(), Token::SqlComment | Token::CComment))
        .map(|token| &hint[token.start as usize..token.end as usize])
        .collect();
    let mut input = TypeSpelling::new(&words);
    let mut components = vec![input.postgres_identifier(false)?];
    while input.eat(".") {
        components.push(input.postgres_identifier(true)?);
    }
    let mut name: FullName = components.join(".").parse()?;
    let special = components.len() == 1 && !components[0].starts_with('"');
    let mut modifiers_parsed = false;
    if special {
        let first = components[0].as_str();
        let builtin = match first {
            "integer" | "int" => Some("int4"),
            "smallint" => Some("int2"),
            "bigint" => Some("int8"),
            "real" => Some("float4"),
            "boolean" => Some("bool"),
            "decimal" | "dec" | "numeric" => Some("numeric"),
            "double" if input.eat("precision") => Some("float8"),
            "float" => {
                modifiers_parsed = true;
                let modifiers = input.modifiers()?;
                let precision = if modifiers.is_empty() {
                    53
                } else {
                    if modifiers.len() != 1 {
                        return Err(invalid_hint());
                    }
                    integer_modifier(&modifiers[0])?
                };
                if !(1..=53).contains(&precision) {
                    return Err(invalid_hint());
                }
                Some(if precision <= 24 { "float4" } else { "float8" })
            }
            "national" | "nchar" | "character" | "char" | "varchar" => {
                if first == "national" && !(input.eat("character") || input.eat("char")) {
                    return Err(invalid_hint());
                }
                Some(if input.eat("varying") || first == "varchar" {
                    "varchar"
                } else {
                    "bpchar"
                })
            }
            "bit" => Some(if input.eat("varying") {
                "varbit"
            } else {
                "bit"
            }),
            "time" | "timestamp" => {
                modifiers_parsed = true;
                input.modifiers()?;
                let with_zone = if input.eat("with") {
                    input.expect("time")?;
                    input.expect("zone")?;
                    true
                } else {
                    if input.eat("without") {
                        input.expect("time")?;
                        input.expect("zone")?;
                    }
                    false
                };
                Some(match (first, with_zone) {
                    ("time", true) => "timetz",
                    ("time", false) => "time",
                    (_, true) => "timestamptz",
                    (_, false) => "timestamp",
                })
            }
            "interval" => {
                modifiers_parsed = true;
                input.modifiers()?;
                input.interval_fields()?;
                Some("interval")
            }
            _ => None,
        };
        if let Some(builtin) = builtin {
            name = FullName::with_schema("pg_catalog", builtin);
        }
    }
    if !modifiers_parsed {
        input.modifiers()?;
    }
    let array = input.postgres_array()?;
    input.finish()?;
    Ok(TypeReference::concrete_type_ref(&name, array))
}

struct TypeSpelling<'a> {
    words: &'a [&'a str],
    position: usize,
}

impl<'a> TypeSpelling<'a> {
    const fn new(words: &'a [&'a str]) -> Self {
        Self { words, position: 0 }
    }

    fn peek(&self) -> Option<&'a str> {
        self.words.get(self.position).copied()
    }

    fn next(&mut self) -> Result<&'a str, AnalysisError> {
        let word = self.peek().ok_or_else(invalid_hint)?;
        self.position += 1;
        Ok(word)
    }

    fn eat(&mut self, expected: &str) -> bool {
        if self
            .peek()
            .is_some_and(|word| word.eq_ignore_ascii_case(expected))
        {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: &str) -> Result<(), AnalysisError> {
        if self.eat(expected) {
            Ok(())
        } else {
            Err(invalid_hint())
        }
    }

    fn finish(&self) -> Result<(), AnalysisError> {
        if self.peek().is_none() {
            Ok(())
        } else {
            Err(invalid_hint())
        }
    }

    fn postgres_identifier(&mut self, qualified: bool) -> Result<String, AnalysisError> {
        let word = self.next()?;
        if word
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("u&\""))
        {
            let escape = if self.eat("uescape") {
                let literal = self.next()?;
                let literal = literal.strip_prefix('\'').ok_or_else(invalid_hint)?;
                let mut chars = literal.strip_suffix('\'').ok_or_else(invalid_hint)?.chars();
                let escape = chars.next().ok_or_else(invalid_hint)?;
                if chars.next().is_some()
                    || escape.is_ascii_hexdigit()
                    || matches!(escape, '+' | '\'' | '"')
                    || escape.is_whitespace()
                {
                    return Err(invalid_hint());
                }
                escape
            } else {
                '\\'
            };
            let decoded = unicode_identifier(&word[2..], escape)?;
            Ok(crate::identifier::QuotedIdentifier::from(decoded.as_str()).to_string())
        } else if word.starts_with('"') {
            let name: FullName = word.parse()?;
            if name.name().as_str().is_empty() {
                return Err(invalid_hint());
            }
            Ok(word.to_owned())
        } else if is_word(word)
            && (qualified || (postgres_word(word) && !word.eq_ignore_ascii_case("setof")))
        {
            Ok(word.to_ascii_lowercase())
        } else {
            Err(invalid_hint())
        }
    }

    // Retain complete modifier arguments, including parenthesized constants.
    // Only float precision affects the type identity here. Validation of other
    // arguments against the type's modifier-input contract is tracked in #690.
    fn modifiers(&mut self) -> Result<Vec<String>, AnalysisError> {
        let mut values = Vec::new();
        if !self.eat("(") {
            return Ok(values);
        }
        let mut depth = 1;
        let mut value = Vec::new();
        loop {
            let word = self.next()?;
            match word {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;
                    if depth == 0 {
                        if value.is_empty() {
                            return Err(invalid_hint());
                        }
                        values.push(modifier_argument(&value)?);
                        return Ok(values);
                    }
                }
                "," if depth == 1 => {
                    if value.is_empty() {
                        return Err(invalid_hint());
                    }
                    values.push(modifier_argument(&value)?);
                    value.clear();
                    continue;
                }
                ";" => return Err(invalid_hint()),
                _ => (),
            }
            value.push(word);
        }
    }

    fn interval_fields(&mut self) -> Result<(), AnalysisError> {
        let Some(field) = self.peek() else {
            return Ok(());
        };
        let field = field.to_ascii_lowercase();
        if !matches!(
            field.as_str(),
            "year" | "month" | "day" | "hour" | "minute" | "second"
        ) {
            return Ok(());
        }
        self.next()?;
        if self.eat("to") {
            let end = self.next()?.to_ascii_lowercase();
            if !matches!(
                (field.as_str(), end.as_str()),
                ("year", "month")
                    | ("day", "hour" | "minute" | "second")
                    | ("hour", "minute" | "second")
                    | ("minute", "second")
            ) {
                return Err(invalid_hint());
            }
            if end == "second" {
                self.modifiers()?;
            }
        } else if field == "second" {
            self.modifiers()?;
        }
        Ok(())
    }

    fn postgres_array(&mut self) -> Result<bool, AnalysisError> {
        let keyword = self.eat("array");
        let mut array = keyword;
        while self.eat("[") {
            array = true;
            if !self.eat("]") {
                let size = self.next()?;
                integer_modifier(size)?;
                self.expect("]")?;
            }
        }
        Ok(array)
    }
}

fn modifier_argument(words: &[&str]) -> Result<String, AnalysisError> {
    let original = words;
    let mut words = words;
    loop {
        if words.first() == Some(&"(") && words.last() == Some(&")") {
            words = &words[1..words.len() - 1];
        } else if matches!(words.first(), Some(&"+" | &"-")) {
            words = &words[1..];
        } else {
            break;
        }
    }
    let Some(value) = words.first() else {
        return Err(invalid_hint());
    };
    let unicode = value
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("u&'"));
    if words.len() != 1
        && !(unicode
            && words.len() == 3
            && words[1].eq_ignore_ascii_case("uescape")
            && words[2].starts_with('\''))
    {
        return Err(invalid_hint());
    }
    let literal = value.starts_with(['\'', '"'])
        || unicode
        || value
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("e'"))
        || (value.starts_with('$') && value.ends_with('$'))
        || value.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || value.parse::<f64>().is_ok();
    if !literal
        && !(is_word(value)
            && (postgres_word(value)
                || ["null", "true", "false"]
                    .iter()
                    .any(|word| value.eq_ignore_ascii_case(word))))
    {
        return Err(invalid_hint());
    }
    Ok(original.join(""))
}

fn postgres_word(word: &str) -> bool {
    use ::pg_query::protobuf::KeywordKind;
    if let Ok(scan) = ::pg_query::scan(word) {
        scan.tokens
            .first()
            .is_some_and(|token| token.keyword_kind() != KeywordKind::ReservedKeyword)
    } else {
        false
    }
}

fn integer_modifier(value: &str) -> Result<u32, AnalysisError> {
    let value = value.replace('_', "");
    let value = value.to_ascii_lowercase();
    let (digits, radix) = if let Some(digits) = value.strip_prefix("0x") {
        (digits, 16)
    } else if let Some(digits) = value.strip_prefix("0o") {
        (digits, 8)
    } else if let Some(digits) = value.strip_prefix("0b") {
        (digits, 2)
    } else {
        (value.as_str(), 10)
    };
    u32::from_str_radix(digits, radix).map_err(|_| invalid_hint())
}

fn is_word(word: &str) -> bool {
    let mut chars = word.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_alphabetic() || !c.is_ascii())
        && chars.all(|c| c == '_' || c == '$' || c.is_alphanumeric() || !c.is_ascii())
}

#[cfg(feature = "sqlite")]
fn sqlite_reference(hint: &str) -> Result<TypeReference, AnalysisError> {
    #[cfg(feature = "workaround_turso_9791")]
    use crate::sqlite_parser::Lexer;
    #[cfg(not(feature = "workaround_turso_9791"))]
    use ::turso_parser::lexer::Lexer;
    use ::turso_parser::token::TokenType;
    let mut words = Vec::new();
    let mut offset = 0;
    while offset < hint.len() {
        if hint.as_bytes()[offset] == b'[' && hint.as_bytes().get(offset + 1) != Some(&b']') {
            let length = hint[offset..].find(']').ok_or_else(invalid_hint)? + 1;
            words.push(&hint[offset..offset + length]);
            offset += length;
            continue;
        }
        let token = Lexer::new(&hint.as_bytes()[offset..])
            .next()
            .ok_or_else(invalid_hint)?
            .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
        let end = offset + token.value.len();
        if token.token_type != TokenType::TK_NONE {
            words.push(&hint[offset..end]);
        }
        offset = end;
    }
    let mut input = TypeSpelling::new(&words);
    let mut names = Vec::new();
    while let Some(word) = input.peek() {
        if is_word(word)
            || word.starts_with(['"', '\'', '`'])
            || (word.starts_with('[') && word.ends_with(']'))
        {
            input.next()?;
            names.push(
                ::turso_parser::ast::Name::from_string(word)
                    .as_str()
                    .to_owned(),
            );
        } else {
            break;
        }
    }
    if names.is_empty() {
        return Err(invalid_hint());
    }
    let modifiers = input.modifiers()?;
    if modifiers.len() > 2 || modifiers.iter().any(|value| value.parse::<f64>().is_err()) {
        return Err(invalid_hint());
    }
    let mut array = false;
    while input.eat("[") {
        input.expect("]")?;
        array = true;
    }
    input.finish()?;
    Ok(TypeReference::concrete_type_ref(
        &FullName::from_local_name(&names.join(" ")),
        array,
    ))
}

fn unicode_identifier(quoted: &str, escape: char) -> Result<String, AnalysisError> {
    let name: FullName = quoted.parse()?;
    let mut chars = name.name().as_str().chars().peekable();
    let mut decoded = String::new();
    while let Some(character) = chars.next() {
        if character != escape {
            decoded.push(character);
            continue;
        }
        if chars.peek() == Some(&escape) {
            chars.next();
            decoded.push(escape);
            continue;
        }
        let width = if chars.peek() == Some(&'+') {
            chars.next();
            6
        } else {
            4
        };
        let mut hex: String = chars.by_ref().take(width).collect();
        if hex.len() != width {
            return Err(invalid_hint());
        }
        let mut code = u32::from_str_radix(&hex, 16).map_err(|_| invalid_hint())?;
        if (0xD800..=0xDBFF).contains(&code) {
            if chars.next() != Some(escape) {
                return Err(invalid_hint());
            }
            hex = chars.by_ref().take(4).collect();
            let low = u32::from_str_radix(&hex, 16).map_err(|_| invalid_hint())?;
            if hex.len() != 4 || !(0xDC00..=0xDFFF).contains(&low) {
                return Err(invalid_hint());
            }
            code = 0x10000 + ((code - 0xD800) << 10) + low - 0xDC00;
        }
        let character = char::from_u32(code)
            .filter(|c| *c != '\0')
            .ok_or_else(invalid_hint)?;
        decoded.push(character);
    }
    Ok(decoded)
}

fn invalid_hint() -> AnalysisError {
    AnalysisError::unexpected_input_error("expected exactly one SQL type name")
}
