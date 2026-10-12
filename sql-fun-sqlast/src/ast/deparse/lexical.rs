use super::{DeparseError, SqlEmit, SqlWriter};
use crate::ast::{
    Dialect, Expr, Literal, Name, OperatorKind, Parameter, SqliteTypeModifiers, SqliteTypeNamePart,
    SqliteTypeNameTrivia, TypeName,
};

impl SqlEmit for Name {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.0.is_empty() {
            return writer.invalid("name", "name has no components");
        }
        writer.names(&self.0, ".")
    }
}

impl Name {
    pub(super) fn is_local(&self, value: &str) -> bool {
        matches!(self.0.as_slice(), [name] if name == value)
    }

    pub(super) fn last_component(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }

    pub(super) fn is_system_function(&self, name: &str) -> bool {
        matches!(self.0.as_slice(), [schema, function] if schema == "pg_catalog" && function == name)
    }

    pub(super) fn emit_operator(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(operator) = self.last_component() else {
            return writer.invalid("operator", "operator name has no components");
        };
        if !valid_operator(operator, writer.dialect()) {
            return writer.invalid("operator", "operator contains invalid punctuation");
        }

        match self.0.as_slice() {
            [_] => writer.sql(operator),
            [schema, _] if writer.is_postgres() => {
                if schema.is_empty() || schema.contains('\0') {
                    return writer.invalid("operator", "operator schema is empty or contains NUL");
                }
                writer.sql("OPERATOR(");
                writer.ident(schema)?;
                writer.sql(".");
                writer.sql(operator);
                writer.sql(")");
            }
            [_schema, _] => {
                return writer.invalid(
                    "operator",
                    "schema-qualified operators are not legal SQLite SQL",
                );
            }
            _ => {
                return writer.invalid(
                    "operator",
                    "schema-qualified operators require one schema component",
                );
            }
        }
        Ok(())
    }
}

fn valid_operator(value: &str, dialect: Dialect) -> bool {
    if dialect == Dialect::sqlite()
        && matches!(value.to_ascii_lowercase().as_str(), "is" | "is not")
    {
        return true;
    }
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| b"+-*/<>=~!@#%^&|`?".contains(&byte))
    {
        return false;
    }
    if dialect == Dialect::Postgres {
        let has_special = value.bytes().any(|byte| b"~!@#%^&|`".contains(&byte));
        !value.contains("--")
            && !value.contains("/*")
            && (value.len() == 1
                || !matches!(value.as_bytes().last(), Some(b'+' | b'-'))
                || has_special)
    } else {
        true
    }
}

impl SqlEmit for Literal {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Null => writer.sql("NULL"),
            Self::Integer(value) => writer.number(value),
            Self::Numeric(value) => {
                if !valid_numeric(value, writer.is_postgres()) {
                    return writer.invalid("numeric literal", "numeric spelling is invalid");
                }
                writer.sql(value);
            }
            Self::String(value) => writer.string(value)?,
            Self::Boolean(value) => writer.sql(if *value { "TRUE" } else { "FALSE" }),
            Self::BitString(value) => {
                if !writer.is_postgres() {
                    return writer.invalid("bit string", "bit strings are not legal SQLite SQL");
                }
                let Some((prefix, digits)) = value.split_at_checked(1) else {
                    return writer.invalid("bit string", "bit string prefix is missing");
                };
                let valid_digits = match prefix {
                    "b" | "B" => digits.bytes().all(|byte| matches!(byte, b'0' | b'1')),
                    "x" | "X" => digits.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    _ => false,
                };
                if !valid_digits {
                    return writer.invalid("bit string", "bit string spelling is invalid");
                }
                writer.sql(prefix);
                writer.sql("'");
                writer.sql(digits);
                writer.sql("'");
            }
            Self::Blob(value) => {
                if writer.is_postgres() {
                    return writer
                        .invalid("blob literal", "blob literals are not legal PostgreSQL SQL");
                }
                if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return writer
                        .invalid("blob literal", "blob payload must be hexadecimal bytes");
                }
                writer.sql("X'");
                writer.sql(value);
                writer.sql("'");
            }
            Self::Keyword(value) => {
                if writer.is_postgres() || !valid_sqlite_keyword_constant(value) {
                    return writer
                        .invalid("keyword literal", "keyword is not a supported SQL constant");
                }
                writer.sql(value);
            }
        }
        Ok(())
    }
}

fn valid_numeric(value: &str, is_postgres: bool) -> bool {
    fn digit_run(text: &str, is_postgres: bool, is_digit: impl Fn(u8) -> bool) -> bool {
        let bytes = text.as_bytes();
        !bytes.is_empty()
            && bytes.iter().enumerate().all(|(index, byte)| {
                if *byte == b'_' && !is_postgres {
                    index > 0
                        && index + 1 < bytes.len()
                        && is_digit(bytes[index - 1])
                        && is_digit(bytes[index + 1])
                } else {
                    is_digit(*byte)
                }
            })
    }

    let value = value.strip_prefix(['+', '-']).unwrap_or(value);
    if value.is_empty() {
        return false;
    }
    if !is_postgres && (value.starts_with("0x") || value.starts_with("0X")) {
        digit_run(&value[2..], is_postgres, |byte| byte.is_ascii_hexdigit())
    } else {
        let (mantissa, exponent) = match value.find(['e', 'E']) {
            Some(index) => (&value[..index], Some(&value[index + 1..])),
            None => (value, None),
        };
        let exponent_is_valid = match exponent {
            Some(exponent) => {
                let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
                digit_run(exponent, is_postgres, |byte| byte.is_ascii_digit())
            }
            None => true,
        };
        let (integer, fraction) = match mantissa.find('.') {
            Some(index) => (&mantissa[..index], Some(&mantissa[index + 1..])),
            None => (mantissa, None),
        };
        let mantissa_is_valid = match fraction {
            Some(fraction) => {
                (integer.is_empty()
                    || digit_run(integer, is_postgres, |byte| byte.is_ascii_digit()))
                    && (fraction.is_empty()
                        || digit_run(fraction, is_postgres, |byte| byte.is_ascii_digit()))
                    && (!integer.is_empty() || !fraction.is_empty())
            }
            None => digit_run(integer, is_postgres, |byte| byte.is_ascii_digit()),
        };
        mantissa_is_valid && exponent_is_valid
    }
}

fn valid_sqlite_keyword_constant(value: &str) -> bool {
    matches!(
        value,
        "DEFAULT"
            | "DELETE"
            | "EXTRA"
            | "FULL"
            | "INCREMENTAL"
            | "MEMORY"
            | "NORMAL"
            | "OFF"
            | "ON"
            | "PASSIVE"
            | "PERSIST"
            | "RESTART"
            | "ROLLBACK"
            | "TRUNCATE"
            | "WAL"
    )
}

impl SqlEmit for Parameter {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.position == 0 {
            return writer.invalid("parameter", "parameter positions start at one");
        }
        // Preserve the binding slot; source names and optional type hints are metadata.
        if writer.is_postgres() {
            writer.sql("$");
        } else {
            // Explicit indices preserve repeated parameters and gaps when new CTEs are emitted.
            writer.sql("?");
        }
        writer.number(self.position);
        Ok(())
    }
}

impl SqlEmit for TypeName {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if !writer.is_postgres() {
            return self.emit_sqlite(writer);
        }

        if self.set_of {
            writer.sql("SETOF ");
        }

        if writer.is_postgres() && self.name.is_system_function("interval") {
            writer.sql("interval");
            emit_interval_modifiers(self, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "timetz") {
            emit_time_zone_type("time", &self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "timestamptz") {
            emit_time_zone_type("timestamp", &self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "bpchar") {
            writer.sql("char");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "bool") {
            writer.sql("boolean");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "int2") {
            writer.sql("smallint");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "int4") {
            writer.sql("int");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "int8") {
            writer.sql("bigint");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres()
            && (writer.is_system_type_name(&self.name, "real")
                || writer.is_system_type_name(&self.name, "float4"))
        {
            writer.sql("real");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "float8") {
            writer.sql("double precision");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "varchar") {
            writer.sql("varchar");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "numeric") {
            writer.sql("numeric");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "time") {
            writer.sql("time");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "timestamp") {
            writer.sql("timestamp");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "varbit") {
            writer.sql("bit varying");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else if writer.is_postgres() && writer.is_system_type_name(&self.name, "bit") {
            writer.sql("bit");
            emit_type_modifiers(&self.modifiers, writer)?;
        } else {
            self.name.emit(writer)?;
            emit_type_modifiers(&self.modifiers, writer)?;
        }

        if writer.is_postgres() {
            for bound in &self.array_bounds {
                writer.sql("[");
                if let Some(bound) = bound {
                    writer.number(bound);
                }
                writer.sql("]");
            }
            if self.percent_type {
                writer.sql("%TYPE");
            }
        }
        Ok(())
    }
}

impl TypeName {
    fn emit_sqlite(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.set_of || self.percent_type || !self.array_bounds.is_empty() {
            return writer.invalid(
                "type name",
                "SETOF, %TYPE, and array bounds are not legal SQLite type syntax",
            );
        }
        let retained_name_matches = self.sqlite_name_snapshot.as_ref() == Some(&self.name);
        if self.sqlite_parts.is_empty() || !retained_name_matches {
            let [name] = self.name.0.as_slice() else {
                return writer.invalid("type name", "SQLite type name must have one component");
            };
            emit_sqlite_type_name_fallback(name, writer)?;
        } else {
            writer.list(&self.sqlite_parts, "")?;
        }
        match &self.sqlite_modifiers {
            Some(modifiers)
                if retained_name_matches && modifiers.values.len() == self.modifiers.len() =>
            {
                emit_retained_sqlite_type_modifiers(modifiers, &self.modifiers, writer)
            }
            _ => emit_sqlite_type_modifiers(&self.modifiers, writer),
        }
    }
}

impl SqlEmit for SqliteTypeNamePart {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        emit_sqlite_type_trivia(&self.leading_trivia, writer)?;
        emit_sqlite_type_word(&self.name, self.quote, writer)?;
        emit_sqlite_type_trivia(&self.trailing_trivia, writer)
    }
}

fn emit_sqlite_type_trivia(
    trivia: &[SqliteTypeNameTrivia],
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    for (index, item) in trivia.iter().enumerate() {
        match item {
            SqliteTypeNameTrivia::Whitespace(value) => {
                if value.contains('\0') || !value.bytes().all(|byte| byte.is_ascii_whitespace()) {
                    return writer
                        .invalid("type name", "SQLite type spacing contains invalid bytes");
                }
                writer.sql(value);
            }
            SqliteTypeNameTrivia::BlockComment(value) => {
                if !value.starts_with("/*")
                    || !value.ends_with("*/")
                    || value.contains('\0')
                    || value.len() < 4
                    || value[2..value.len() - 2].contains("*/")
                {
                    return writer.invalid("type name", "SQLite type comment is malformed");
                }
                writer.sql(value);
            }
            SqliteTypeNameTrivia::LineComment(value) => {
                if !value.starts_with("--")
                    || value.contains('\0')
                    || value.contains('\r')
                    || value.contains('\n')
                {
                    return writer.invalid("type name", "SQLite type comment is malformed");
                }
                writer.sql(value);
                let newline_follows = trivia.get(index + 1).is_some_and(|next| {
                    matches!(next, SqliteTypeNameTrivia::Whitespace(whitespace)
                        if whitespace.contains('\r') || whitespace.contains('\n'))
                });
                if !newline_follows {
                    writer.sql("\n");
                }
            }
        }
    }
    Ok(())
}

fn emit_retained_sqlite_type_modifiers(
    modifiers: &SqliteTypeModifiers,
    expressions: &[Expr],
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    if modifiers.values.len() != expressions.len()
        || modifiers.ast_spellings.len() != expressions.len()
        || modifiers.values.len() > 2
    {
        return writer.invalid(
            "type modifier",
            "SQLite type modifiers do not match the AST",
        );
    }
    writer.sql("(");
    for (index, ((modifier, expression), original_ast_spelling)) in modifiers
        .values
        .iter()
        .zip(expressions)
        .zip(&modifiers.ast_spellings)
        .enumerate()
    {
        emit_sqlite_type_trivia(&modifier.leading_trivia, writer)?;
        let Some(current_ast_spelling) = expression.sqlite_type_modifier_spelling() else {
            return writer.invalid(
                "type modifier",
                "SQLite type modifiers must be signed numeric literals",
            );
        };
        if &current_ast_spelling == original_ast_spelling
            && valid_numeric(&modifier.spelling, false)
        {
            writer.sql(&modifier.spelling);
        } else {
            writer.sql(&current_ast_spelling);
        }
        emit_sqlite_type_trivia(&modifier.trailing_trivia, writer)?;
        if index + 1 < expressions.len() {
            writer.sql(",");
        }
    }
    writer.sql(")");
    emit_sqlite_type_trivia(&modifiers.trailing_trivia, writer)
}

fn emit_sqlite_type_name_fallback(name: &str, writer: &mut SqlWriter) -> Result<(), DeparseError> {
    let bytes = name.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let whitespace_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        writer.sql(&name[whitespace_start..cursor]);
        if cursor == bytes.len() {
            break;
        }
        let word_start = cursor;
        while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        emit_sqlite_type_word(&name[word_start..cursor], None, writer)?;
    }
    if name.is_empty() {
        writer.sql("\"\"");
    }
    Ok(())
}

fn emit_sqlite_type_word(
    name: &str,
    quote: Option<char>,
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    if name.contains('\0') {
        return writer.invalid("type name", "SQLite type name contains a NUL byte");
    }
    if let Some(quote) = quote {
        if quote == '[' && name.contains(']') {
            return writer.ident(name);
        }
        let (open, close) = if quote == '[' {
            ('[', ']')
        } else {
            (quote, quote)
        };
        writer.sql(&open.to_string());
        for character in name.chars() {
            writer.sql(&character.to_string());
            if character == close && quote != '[' {
                writer.sql(&character.to_string());
            }
        }
        writer.sql(&close.to_string());
    } else if valid_sqlite_type_word(name) {
        writer.sql(name);
    } else {
        writer.ident(name)?;
    }
    Ok(())
}

fn valid_sqlite_type_word(name: &str) -> bool {
    const fn is_start(byte: u8) -> bool {
        byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
    }

    const fn is_continue(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') || byte >= 0x80
    }

    let bytes = name.as_bytes();
    bytes.first().is_some_and(|byte| is_start(*byte))
        && bytes[1..].iter().all(|byte| is_continue(*byte))
}

impl SqlWriter {
    fn is_system_type_name(&self, name: &Name, value: &str) -> bool {
        self.is_postgres() && name.is_system_function(value)
    }
}

impl Expr {
    pub(in crate::ast) const fn integer_literal(&self) -> Option<i32> {
        match self {
            Self::Literal(Literal::Integer(value)) => Some(*value),
            _ => None,
        }
    }
}

fn emit_type_modifiers(modifiers: &[Expr], writer: &mut SqlWriter) -> Result<(), DeparseError> {
    if !modifiers.is_empty() {
        writer.sql("(");
        writer.list(modifiers, ", ")?;
        writer.sql(")");
    }
    Ok(())
}

fn emit_sqlite_type_modifiers(
    modifiers: &[Expr],
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    if modifiers.len() > 2 {
        return writer.invalid(
            "type modifier",
            "SQLite type names accept at most two numeric modifiers",
        );
    }
    if modifiers.is_empty() {
        return Ok(());
    }

    writer.sql("(");
    for (index, modifier) in modifiers.iter().enumerate() {
        if index > 0 {
            writer.sql(",");
        }
        modifier.emit_sqlite_type_modifier(writer)?;
    }
    writer.sql(")");
    Ok(())
}

fn emit_time_zone_type(
    base: &str,
    modifiers: &[Expr],
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    writer.sql(base);
    emit_type_modifiers(modifiers, writer)?;
    writer.sql(" with time zone");
    Ok(())
}

fn emit_interval_modifiers(
    type_name: &TypeName,
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    match type_name.modifiers.as_slice() {
        [] => Ok(()),
        [mask] => {
            let Some(mask) = mask.integer_literal() else {
                return writer.invalid("interval type", "interval range mask must be an integer");
            };
            emit_interval_range(mask, writer)
        }
        [mask, precision] => {
            let Some(mask) = mask.integer_literal() else {
                return writer.invalid("interval type", "interval range mask must be an integer");
            };
            let Some(precision) = precision.integer_literal() else {
                return writer.invalid("interval type", "interval precision must be an integer");
            };
            emit_interval_range(mask, writer)?;
            if precision != 65_535 {
                if !(0..=6).contains(&precision) {
                    return writer.invalid(
                        "interval type",
                        "interval precision must be between zero and six",
                    );
                }
                writer.sql("(");
                writer.number(precision);
                writer.sql(")");
            }
            Ok(())
        }
        _ => writer.invalid(
            "interval type",
            "interval type has an unsupported modifier shape",
        ),
    }
}

fn emit_interval_range(mask: i32, writer: &mut SqlWriter) -> Result<(), DeparseError> {
    let range = match mask {
        4 => " year",
        2 => " month",
        8 => " day",
        1_024 => " hour",
        2_048 => " minute",
        4_096 => " second",
        6 => " year to month",
        1_032 => " day to hour",
        3_080 => " day to minute",
        7_176 => " day to second",
        3_072 => " hour to minute",
        7_168 => " hour to second",
        6_144 => " minute to second",
        32_767 => "",
        _ => return writer.invalid("interval type", "interval range mask is unsupported"),
    };
    writer.sql(range);
    Ok(())
}

#[cfg(test)]
mod test_lexical_emit {
    use super::{Name, Parameter, SqlEmit, SqlWriter, TypeName};
    use crate::ast::{
        Dialect, Expr, Literal, SqliteTypeModifier, SqliteTypeModifiers, SqliteTypeNamePart,
        SqliteTypeNameTrivia,
    };

    #[cfg(feature = "sqlite")]
    fn sqlite_statement(sql: &str) -> crate::ast::Statement {
        crate::parse_statements(sql, Dialect::sqlite())
            .next()
            .expect("one statement")
            .expect(sql)
    }

    fn render<T: SqlEmit>(value: &T, dialect: Dialect) -> Result<String, super::DeparseError> {
        let mut writer = SqlWriter::new(dialect);
        value.emit(&mut writer)?;
        Ok(writer.sql)
    }

    #[rstest::rstest]
    #[case::quoted_component(vec!["select".to_owned(), "a\"b".to_owned()], Dialect::postgres(), "\"select\".\"a\"\"b\"")]
    #[case::sqlite_quoted_component(vec!["main".to_owned(), "odd name".to_owned()], Dialect::sqlite(), "\"main\".\"odd name\"")]
    fn quotes_each_name_component(
        #[case] parts: Vec<String>,
        #[case] dialect: Dialect,
        #[case] expected: &str,
    ) {
        assert_eq!(
            render(&Name::from_parts(&parts), dialect).unwrap(),
            expected
        );
    }

    #[test]
    fn emits_schema_qualified_postgres_operators_with_operator_syntax() {
        let operator = Name::from_parts(&["math".to_owned(), "+".to_owned()]);
        let mut writer = SqlWriter::new(Dialect::postgres());
        operator.emit_operator(&mut writer).unwrap();
        assert_eq!(writer.sql, "OPERATOR(\"math\".+)");
    }

    #[rstest::rstest]
    #[case::single_quote("it's", Dialect::sqlite(), "'it''s'")]
    #[case::backslash("a\\b'c", Dialect::postgres(), "E'a\\\\b''c'")]
    fn escapes_string_values(
        #[case] value: &str,
        #[case] dialect: Dialect,
        #[case] expected: &str,
    ) {
        assert_eq!(
            render(&Literal::String(value.to_owned()), dialect).unwrap(),
            expected
        );
    }

    #[rstest::rstest]
    #[case::postgres_gap(3, Dialect::postgres(), "$3")]
    #[case::sqlite_gap(3, Dialect::sqlite(), "?3")]
    #[case::sqlite_numbered_parameter(1, Dialect::sqlite(), "?1")]
    fn preserves_explicit_parameter_positions(
        #[case] position: u32,
        #[case] dialect: Dialect,
        #[case] expected: &str,
    ) {
        let parameter = Parameter {
            position,
            name: None,
            type_name: None,
        };
        assert_eq!(render(&parameter, dialect).unwrap(), expected);
    }

    #[test]
    fn preserves_repeated_sqlite_parameter_identity() {
        let parameter = Parameter {
            position: 4,
            name: None,
            type_name: None,
        };
        let mut writer = SqlWriter::new(Dialect::sqlite());
        writer.list(&[parameter.clone(), parameter], ", ").unwrap();
        assert_eq!(writer.sql, "?4, ?4");
    }

    #[test]
    fn renders_sqlite_parameter_metadata_by_its_numbered_slot() {
        let parameter = Parameter {
            position: 2,
            name: Some(":named".to_owned()),
            type_name: Some("INTEGER".to_owned()),
        };
        assert_eq!(render(&parameter, Dialect::sqlite()).unwrap(), "?2");
    }

    #[rstest::rstest]
    #[case::integer("1", Dialect::postgres())]
    #[case::fraction(".5", Dialect::postgres())]
    #[case::exponent("1.25e-3", Dialect::sqlite())]
    #[case::postgres_negative_fraction("-1.25", Dialect::postgres())]
    #[case::postgres_negative_exponent("-1e-9", Dialect::postgres())]
    #[case::postgres_positive_fraction("+.5", Dialect::postgres())]
    #[case::sqlite_hex("0xCAFE", Dialect::sqlite())]
    #[case::sqlite_hex_separators("0xCA_FE", Dialect::sqlite())]
    #[case::sqlite_decimal_separators("1_000", Dialect::sqlite())]
    #[case::sqlite_fraction_and_exponent_separators("1_0.0_5e-1_0", Dialect::sqlite())]
    fn preserves_valid_numeric_spellings(#[case] value: &str, #[case] dialect: Dialect) {
        assert_eq!(
            render(&Literal::Numeric(value.to_owned()), dialect).unwrap(),
            value
        );
    }

    #[rstest::rstest]
    #[case::postgres_multiple_signs("--1", Dialect::postgres())]
    #[case::postgres_bad_exponent("1e+", Dialect::postgres())]
    #[case::postgres_sql_injection("1; SELECT 1", Dialect::postgres())]
    #[case::postgres_underscore("1_000", Dialect::postgres())]
    #[case::sqlite_repeated_separator("1__0", Dialect::sqlite())]
    #[case::sqlite_trailing_separator("1_", Dialect::sqlite())]
    #[case::sqlite_leading_separator("_1", Dialect::sqlite())]
    #[case::sqlite_bad_exponent_separator("1e_2", Dialect::sqlite())]
    #[case::sqlite_bad_hex_separator("0x_FF", Dialect::sqlite())]
    fn rejects_malformed_numeric_spellings(#[case] value: &str, #[case] dialect: Dialect) {
        let error = render(&Literal::Numeric(value.to_owned()), dialect).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
        assert_eq!(error.node(), "numeric literal", "{error}");
    }

    #[rstest::rstest]
    #[case("SELECT -1.25")]
    #[case("SELECT -1e-9")]
    #[case("SELECT +.5")]
    #[case("SELECT 2 - -1.25")]
    #[case("SELECT CAST(-1.25 AS numeric)")]
    fn postgres_signed_float_expressions_reparse_stably(#[case] sql: &str) {
        let first = crate::parse_statements(sql, Dialect::postgres())
            .next()
            .expect("one statement")
            .expect(sql)
            .deparse_sql(Dialect::postgres())
            .unwrap();
        let second = crate::parse_statements(&first, Dialect::postgres())
            .next()
            .expect("one statement after deparse")
            .expect(&first)
            .deparse_sql(Dialect::postgres())
            .unwrap();
        assert_eq!(first, second, "{sql} => {first}");
    }

    #[test]
    fn emits_postgres_bit_string_prefix_and_digits() {
        assert_eq!(
            render(&Literal::BitString("b101".to_owned()), Dialect::postgres()).unwrap(),
            "b'101'"
        );
        assert_eq!(
            render(&Literal::BitString("xCAFE".to_owned()), Dialect::postgres()).unwrap(),
            "x'CAFE'"
        );
    }

    #[test]
    fn emits_sqlite_blob_payload_as_hex() {
        assert_eq!(
            render(&Literal::Blob("aBcD".to_owned()), Dialect::sqlite()).unwrap(),
            "X'aBcD'"
        );
    }

    #[rstest::rstest]
    #[case::sqlite_multword(vec!["DOUBLE PRECISION".to_owned()], Dialect::sqlite(), "DOUBLE PRECISION")]
    #[case::postgres_canonical(vec!["pg_catalog".to_owned(), "int4".to_owned()], Dialect::postgres(), "int")]
    fn emits_type_names_with_dialect_spelling(
        #[case] parts: Vec<String>,
        #[case] dialect: Dialect,
        #[case] expected: &str,
    ) {
        let value = TypeName {
            name: Name::from_parts(&parts),
            modifiers: Vec::new(),
            array_bounds: Vec::new(),
            set_of: false,
            percent_type: false,
            sqlite_name_snapshot: None,
            sqlite_parts: Vec::new(),
            sqlite_modifiers: None,
            location: None,
        };
        assert_eq!(render(&value, dialect).unwrap(), expected);
    }

    #[test]
    fn emits_interval_range_and_precision() {
        let value = TypeName {
            name: Name::from_parts(&["pg_catalog".to_owned(), "interval".to_owned()]),
            modifiers: vec![
                Expr::Literal(Literal::Integer(7_176)),
                Expr::Literal(Literal::Integer(3)),
            ],
            array_bounds: Vec::new(),
            set_of: false,
            percent_type: false,
            sqlite_name_snapshot: None,
            sqlite_parts: Vec::new(),
            sqlite_modifiers: None,
            location: None,
        };
        assert_eq!(
            render(&value, Dialect::postgres()).unwrap(),
            "interval day to second(3)"
        );
    }

    #[test]
    fn retained_sqlite_type_syntax_tracks_current_ast_values() {
        let mut value = TypeName {
            name: Name::local("NUMERIC"),
            modifiers: vec![Expr::Literal(Literal::Integer(10))],
            array_bounds: Vec::new(),
            set_of: false,
            percent_type: false,
            sqlite_name_snapshot: Some(Name::local("NUMERIC")),
            sqlite_parts: vec![SqliteTypeNamePart {
                name: "NUMERIC".to_owned(),
                quote: Some('"'),
                leading_trivia: Vec::new(),
                trailing_trivia: Vec::new(),
            }],
            sqlite_modifiers: Some(SqliteTypeModifiers {
                values: vec![SqliteTypeModifier {
                    leading_trivia: Vec::new(),
                    spelling: "1_0".to_owned(),
                    trailing_trivia: Vec::new(),
                }],
                ast_spellings: vec!["10".to_owned()],
                trailing_trivia: Vec::new(),
            }),
            location: None,
        };
        assert_eq!(
            render(&value, Dialect::sqlite()).unwrap(),
            "\"NUMERIC\"(1_0)"
        );

        value.modifiers[0] = Expr::Literal(Literal::Integer(11));
        assert_eq!(
            render(&value, Dialect::sqlite()).unwrap(),
            "\"NUMERIC\"(11)"
        );

        value.name = Name::local("TEXT");
        assert_eq!(render(&value, Dialect::sqlite()).unwrap(), "TEXT(11)");

        let mut malformed = value.clone();
        malformed.name = Name::local("NUMERIC");
        malformed.sqlite_parts[0].leading_trivia =
            vec![SqliteTypeNameTrivia::BlockComment("/*/".to_owned())];
        malformed.sqlite_name_snapshot = Some(Name::local("NUMERIC"));
        let error = render(&malformed, Dialect::sqlite()).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
        assert_eq!(error.node(), "type name", "{error}");
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case("SELECT CAST('1.25' AS TEXT)")]
    #[case("SELECT CAST('1.25' AS DOUBLE PRECISION)")]
    #[case(r#"SELECT CAST('1.25' AS "My Type")"#)]
    #[case(r#"SELECT CAST('1.25' AS "quote""type")"#)]
    #[case("SELECT CAST('1.25' AS [Double  Precision])")]
    #[case("SELECT CAST('1.25' AS NUMERIC(10,-2))")]
    #[case(r#"SELECT CAST('1.25' AS "NUMERIC"(10,-2))"#)]
    #[case("SELECT CAST('1.25' AS [My-Type])")]
    fn sqlite_cast_type_names_are_stable_after_reparsing(#[case] sql: &str) {
        let first = sqlite_statement(sql)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        let second = sqlite_statement(&first)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        assert_eq!(first, second, "{sql} => {first}");
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_declared_type_spelling_and_cast_affinity_survive_emission() {
        let create = r#"CREATE TABLE t (a TEXT, b DOUBLE PRECISION, c UNSIGNED BIG INT, d "My Type", e "quote""type", f [Double  Precision], g NUMERIC(10,-2), h "NUMERIC"(10,-2), i custom$kind, j emoji🧪type, k [My-Type])"#;
        let emitted = sqlite_statement(create)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        assert_eq!(
            declared_types(create),
            declared_types(&emitted),
            "{create} => {emitted}"
        );

        let casts = r#"SELECT typeof(CAST('12.5' AS TEXT)), typeof(CAST('12.5' AS DOUBLE PRECISION)), typeof(CAST('12.5' AS UNSIGNED BIG INT)), typeof(CAST('12.5' AS "My Type")), typeof(CAST('12.5' AS [Double  Precision]))"#;
        let emitted_casts = sqlite_statement(casts)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        assert_eq!(sqlite_cast_types(casts), sqlite_cast_types(&emitted_casts));
        assert_eq!(
            sqlite_cast_types(&emitted_casts),
            (
                "text".to_owned(),
                "real".to_owned(),
                "integer".to_owned(),
                "real".to_owned(),
                "real".to_owned(),
            )
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_mixed_quoted_type_words_preserve_declared_type_and_affinity() {
        let create = r#"CREATE TABLE t (
            a DOUBLE "PRECISION"(3),
            b "DOUBLE" PRECISION(3),
            c "BLOB" INTEGER,
            d "",
            e [BLOB] INT,
            f BLOB/*TEXT*/REAL,
            g BLOB(/*INT*/10),
            h BLOB /*INT*/ (10),
            i NUMERIC(10/*INT*/, --TEXT
                -2)
        )"#;
        let emitted = sqlite_statement(create)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        assert_eq!(
            declared_types(create),
            declared_types(&emitted),
            "{create} => {emitted}"
        );
        let columns = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
        assert_eq!(
            sqlite_inserted_types(create, &columns),
            sqlite_inserted_types(&emitted, &columns),
            "{create} => {emitted}"
        );
        let reparsed = sqlite_statement(&emitted)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        assert_eq!(emitted, reparsed, "{create} => {emitted}");
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_numeric_separators_are_preserved_and_execute() {
        let sql = "SELECT 1_000, 1_0.0_5e-1_0, 0xCA_FE";
        let emitted = sqlite_statement(sql)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        assert!(emitted.contains("1_000"), "{emitted}");
        assert!(emitted.contains("1_0.0_5e-1_0"), "{emitted}");
        assert!(emitted.contains("0xCA_FE"), "{emitted}");
        assert_eq!(sqlite_numeric_values(sql), sqlite_numeric_values(&emitted));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_signed_numeric_type_modifiers_are_legal_and_nonconstants_fail() {
        let sql = "SELECT typeof(CAST('12.5' AS NUMERIC(10,-2)))";
        let emitted = sqlite_statement(sql)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        let reparsed = sqlite_statement(&emitted)
            .deparse_sql(Dialect::sqlite())
            .unwrap();
        assert_eq!(emitted, reparsed);
        assert_eq!(sqlite_single_type(sql), "real");
        assert_eq!(sqlite_single_type(&emitted), "real");

        let forged = TypeName {
            name: Name::local("NUMERIC"),
            modifiers: vec![Expr::Column(Name::local("column_name"))],
            array_bounds: Vec::new(),
            set_of: false,
            percent_type: false,
            sqlite_name_snapshot: None,
            sqlite_parts: Vec::new(),
            sqlite_modifiers: None,
            location: None,
        };
        let error = render(&forged, Dialect::sqlite()).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
        assert_eq!(error.node(), "type modifier", "{error}");
    }

    #[cfg(feature = "sqlite")]
    fn declared_types(sql: &str) -> Vec<String> {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch(sql).unwrap();
        let mut statement = connection.prepare("PRAGMA table_info(t)").unwrap();
        statement
            .query_map([], |row| row.get(2))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[cfg(feature = "sqlite")]
    fn sqlite_inserted_types(sql: &str, columns: &[&str]) -> Vec<String> {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch(sql).unwrap();
        let values = std::iter::repeat_n("12.0", columns.len())
            .collect::<Vec<_>>()
            .join(", ");
        connection
            .execute_batch(&format!("INSERT INTO t VALUES ({values})"))
            .unwrap();
        let expressions = columns
            .iter()
            .map(|column| format!("typeof({column})"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = connection
            .prepare(&format!("SELECT {expressions} FROM t"))
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();
        (0..columns.len())
            .map(|index| row.get(index).unwrap())
            .collect()
    }

    #[cfg(feature = "sqlite")]
    fn sqlite_cast_types(sql: &str) -> (String, String, String, String, String) {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .query_row(sql, [], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
    }

    #[cfg(feature = "sqlite")]
    fn sqlite_single_type(sql: &str) -> String {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.query_row(sql, [], |row| row.get(0)).unwrap()
    }

    #[cfg(feature = "sqlite")]
    fn sqlite_numeric_values(sql: &str) -> (i64, f64, i64) {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .query_row(sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
    }
}

impl Expr {
    pub(in crate::ast) const fn boolean_literal(&self) -> Option<bool> {
        match self {
            Self::Literal(Literal::Boolean(value)) => Some(*value),
            _ => None,
        }
    }
    pub(in crate::ast) const fn is_null_literal(&self) -> bool {
        matches!(self, Self::Literal(Literal::Null))
    }

    fn emit_sqlite_type_modifier(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(spelling) = self.sqlite_type_modifier_spelling() else {
            return writer.invalid(
                "type modifier",
                "SQLite type modifiers must be signed numeric literals",
            );
        };
        writer.sql(&spelling);
        Ok(())
    }

    pub(in crate::ast) fn sqlite_type_modifier_spelling(&self) -> Option<String> {
        match self {
            Self::Literal(Literal::Integer(value)) => Some(value.to_string()),
            Self::Literal(Literal::Numeric(value)) if valid_numeric(value, false) => {
                Some(value.clone())
            }
            Self::Operator(operator) => {
                let Some(sign @ ("+" | "-")) = operator.name.last_component() else {
                    return None;
                };
                if !matches!(operator.kind, OperatorKind::Ordinary)
                    || operator.left.is_some()
                    || operator.escape.is_some()
                {
                    return None;
                }
                match operator.right.as_ref() {
                    Some(Self::Literal(Literal::Integer(value))) => Some(format!("{sign}{value}")),
                    Some(Self::Literal(Literal::Numeric(value))) if valid_numeric(value, false) => {
                        Some(format!("{sign}{value}"))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
