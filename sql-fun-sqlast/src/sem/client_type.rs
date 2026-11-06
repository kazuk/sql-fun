use crate::{
    ast::client_type::{ClientBuiltin, ClientTypeHint, ClientTypeName, ClientTypePath},
    names::FullName,
    sem::{AnalysisError, ParseContext, TypeReference},
};

fn resolve_named_type(path: ClientTypePath) -> FullName {
    match path {
        ClientTypePath::Local(name) => FullName::from_local_name(&name),
        ClientTypePath::Schema(schema, name) => FullName::with_schema(&schema, &name),
        ClientTypePath::Catalog(catalog, schema, name) => {
            FullName::with_catalog(&catalog, &schema, &name)
        }
    }
}

const fn builtin_identity(name: ClientBuiltin) -> &'static str {
    match name {
        ClientBuiltin::Integer => "int4",
        ClientBuiltin::Smallint => "int2",
        ClientBuiltin::Bigint => "int8",
        ClientBuiltin::Real => "float4",
        ClientBuiltin::Boolean => "bool",
        ClientBuiltin::Numeric => "numeric",
        ClientBuiltin::DoublePrecision => "float8",
        ClientBuiltin::Character => "bpchar",
        ClientBuiltin::Varchar => "varchar",
        ClientBuiltin::Bit => "bit",
        ClientBuiltin::BitVarying => "varbit",
        ClientBuiltin::Time => "time",
        ClientBuiltin::TimeWithZone => "timetz",
        ClientBuiltin::Timestamp => "timestamp",
        ClientBuiltin::TimestampWithZone => "timestamptz",
        ClientBuiltin::Interval => "interval",
        ClientBuiltin::Json => "json",
    }
}

fn resolve_builtin_type(name: ClientBuiltin) -> FullName {
    FullName::with_schema("pg_catalog", builtin_identity(name))
}

fn resolve_float_type(precision: Option<u32>) -> Result<FullName, AnalysisError> {
    let identity = if let Some(precision) = precision {
        if !(1..=53).contains(&precision) {
            return Err(invalid("float precision must be between 1 and 53"));
        }
        if precision <= 24 { "float4" } else { "float8" }
    } else {
        "float8"
    };
    Ok(FullName::with_schema("pg_catalog", identity))
}

#[cfg(feature = "sqlite")]
fn resolve_sqlite_type(parts: &[String]) -> FullName {
    FullName::from_local_name(&parts.join(" "))
}

fn resolve_client_type_name(name: ClientTypeName) -> Result<FullName, AnalysisError> {
    match name {
        ClientTypeName::Named(path) => Ok(resolve_named_type(path)),
        ClientTypeName::Builtin(name) => Ok(resolve_builtin_type(name)),
        ClientTypeName::Float(precision) => resolve_float_type(precision),
        #[cfg(feature = "sqlite")]
        ClientTypeName::Sqlite(parts) => Ok(resolve_sqlite_type(&parts)),
        ClientTypeName::UnsupportedModifier => Err(typmod()),
    }
}

/// Interpret one SQL type name for a client-protocol parameter hint.
///
/// The context's explicitly selected dialect controls SQL quoting, case folding,
/// aliases, and array syntax. The result retains named identity only: parsing
/// neither looks up the catalog nor applies a search path. Callers can resolve
/// it with their context's `canonicalize_type_ref` afterwards.
///
/// PostgreSQL type modifiers cannot be represented by protocol parameter OIDs
/// and are rejected, including interval field restrictions. `float(p)` selects
/// `float4` or `float8` and is accepted. Bare `character` and `bit` select their
/// type identity without imposing the length used by SQL column declarations.
/// SQLite declarations retain their logical name, without PostgreSQL aliases.
/// This SQL entry point does not change canonical `FromStr` or serde formats.
///
/// # Errors
/// Returns an error for malformed or trailing input, unsupported type modifiers,
/// or SQLite syntax when the `sqlite` feature is unavailable. A syntactically
/// valid name can succeed even when it does not exist in the catalog.
pub fn parse_client_type(
    input: &str,
    context: &impl ParseContext,
) -> Result<TypeReference, AnalysisError> {
    let (name, array) = ClientTypeHint::parse(input, context.dialect())?.into_parts();
    let name = resolve_client_type_name(name)?;
    Ok(TypeReference::concrete_type_ref(&name, array))
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::Dialect,
        names::FullName,
        sem::{TypeReference, parse_client_type},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::integer("INTEGER", "int4")]
    #[case::int("int", "int4")]
    #[case::smallint("SMALLINT", "int2")]
    #[case::bigint("bigint", "int8")]
    #[case::real("real", "float4")]
    #[case::double("double precision", "float8")]
    #[case::float("float", "float8")]
    #[case::float_min("float(1)", "float4")]
    #[case::float_short("float(24)", "float4")]
    #[case::float_long("float(25)", "float8")]
    #[case::float_max("float(53)", "float8")]
    #[case::float_hex("float(0x18)", "float4")]
    #[case::float_binary("float(0b11000)", "float4")]
    #[case::float_octal("float(0o30)", "float4")]
    #[case::float_separator("float(5_3)", "float8")]
    #[case::float_array("float(24)[2][]", "_float4")]
    #[case::boolean("BOOLEAN", "bool")]
    #[case::numeric("numeric", "numeric")]
    #[case::decimal("decimal", "numeric")]
    #[case::dec("dec", "numeric")]
    #[case::character("character", "bpchar")]
    #[case::char("char", "bpchar")]
    #[case::varchar("VARCHAR", "varchar")]
    #[case::varying("character varying", "varchar")]
    #[case::char_varying("char varying", "varchar")]
    #[case::national("national character", "bpchar")]
    #[case::national_char("national char varying", "varchar")]
    #[case::national_varying("national character varying", "varchar")]
    #[case::nchar("nchar", "bpchar")]
    #[case::nchar_varying("nchar varying", "varchar")]
    #[case::bit("bit", "bit")]
    #[case::bit_varying("bit varying", "varbit")]
    #[case::time("time", "time")]
    #[case::timestamp("timestamp", "timestamp")]
    #[case::time_with("time with time zone", "timetz")]
    #[case::time_without("time without time zone", "time")]
    #[case::timestamp_with("timestamp with time zone", "timestamptz")]
    #[case::timestamp_without("timestamp without time zone", "timestamp")]
    #[case::interval("interval", "interval")]
    #[case::json("json", "json")]
    #[case::comments(" /* nested /* comment */ */ DOUBLE -- line\n PRECISION ", "float8")]
    fn interprets_postgres_builtin_identity(#[case] input: &str, #[case] expected: &str) {
        let context = TestParseContext::default().with_dialect(Dialect::postgres());
        assert_eq!(
            parse_client_type(input, &context).unwrap(),
            TypeReference::from_master_type_name(expected)
        );
    }

    #[rstest::rstest]
    #[case::unqualified("InT4", & ["int4"])]
    #[case::quoted(r#""INTEGER""#, & ["INTEGER"])]
    #[case::qualified_alias("pg_catalog.integer", & ["pg_catalog", "integer"])]
    #[case::qualified("PG_CATALOG.INT4", & ["pg_catalog", "int4"])]
    #[case::catalog("db.public.Kind", & ["db", "public", "kind"])]
    #[case::quoted_qualified(r#""My.Schema"."My.Type""#, & ["My.Schema", "My.Type"])]
    #[case::keyword_label("public.select", & ["public", "select"])]
    #[case::type_keyword("authorization", & ["authorization"])]
    #[case::unreserved("vacuum", & ["vacuum"])]
    #[case::escaped_quote(r#""a""b""#, & ["a\"b"])]
    #[case::unicode(r#"U&"int\0034""#, & ["int4"])]
    #[case::unicode_qualified(r#"U&"pg\005fcatalog".U&"int\0034""#, & ["pg_catalog", "int4"])]
    #[case::unicode_escape_string(r#"U&"int!0034" UESCAPE E'!'"#, &["int4"])]
    #[case::unicode_octal_escape(r#"U&"int!0034" UESCAPE E'\041'"#, &["int4"])]
    #[case::unicode_hex_escape(r#"U&"int!0034" UESCAPE E'\x21'"#, &["int4"])]
    #[case::unicode_literal_x_escape(r#"U&"intx0034" UESCAPE E'\x'"#, &["int4"])]
    #[case::unicode_escape_code(r#"U&"int!0034" UESCAPE E'\u0021'"#, &["int4"])]
    #[case::unicode_long_escape_code(r#"U&"int!0034" UESCAPE E'\U00000021'"#, &["int4"])]
    #[case::unicode_continuation(concat!(r#"U&"int!0034" UESCAPE '!'"#, "\n", "''"), &["int4"])]
    #[case::unicode_cr_comment_continuation(concat!(r#"U&"int!0034" UESCAPE '!'"#, "\r-- continuation\r''"), &["int4"])]
    #[case::unicode_comment_continuation(concat!(r#"U&"int!0034" UESCAPE '!'"#, "\n-- continuation\n''"), &["int4"])]
    #[case::unicode_custom(r#"U&"a!002eb" UESCAPE '!'"#, & ["a.b"])]
    #[case::unicode_comments(r#"U&"int!0034" /* c */ UESCAPE /* c */ '!'"#, & ["int4"])]
    #[case::unicode_case(r#"u&"Mi\0058ed""#, & ["MiXed"])]
    #[case::unicode_six(r#"U&"\+01f600""#, & ["😀"])]
    #[case::unicode_surrogate(r#"U&"\d83d\de00""#, & ["😀"])]
    #[case::unicode_quote(r#"U&"a""b""#, & ["a\"b"])]
    #[case::unicode_literal_escape(r#"U&"a\\b""#, & [r"a\b"])]
    #[case::literal_suffix(r#""kind[]""#, & ["kind[]"])]
    #[case::literal_modifier(r#""numeric(5,2)""#, & ["numeric(5,2)"])]
    fn parses_names_without_catalog_lookup(#[case] input: &str, #[case] parts: &[&str]) {
        let context = TestParseContext::default().with_dialect(Dialect::postgres());
        let expected = match parts {
            [name] => FullName::from_local_name(name),
            [schema, name] => FullName::with_schema(schema, name),
            [catalog, schema, name] => FullName::with_catalog(catalog, schema, name),
            _ => unreachable!(),
        };
        assert_eq!(
            parse_client_type(input, &context).unwrap(),
            TypeReference::from_full_name(expected)
        );
    }

    #[rstest::rstest]
    #[case::one("integer[]")]
    #[case::repeated("integer[][]")]
    #[case::bounded("integer[2][3]")]
    #[case::mixed("integer[2][][0]")]
    #[case::spaces("integer [ 2 ] [ ]")]
    #[case::array_keyword("integer ARRAY")]
    #[case::array_bound("integer ARRAY[4]")]
    #[case::array_hex("integer[0x10]")]
    fn arrays_share_one_type_identity(#[case] input: &str) {
        let context = TestParseContext::default().with_dialect(Dialect::postgres());
        assert_eq!(
            parse_client_type(input, &context).unwrap(),
            TypeReference::from_master_type_name("_int4")
        );
    }

    #[rstest::rstest]
    #[case::varchar("character varying(100)")]
    #[case::numeric("numeric(10,2)")]
    #[case::timestamp("timestamp(2) with time zone")]
    #[case::array("numeric(10,2)[][]")]
    #[case::arbitrary("public.custom(5)")]
    #[case::invalid_semantic("int4(5)")]
    #[case::interval_precision("interval(2)")]
    #[case::interval_field("interval day to second")]
    #[case::interval_second("interval second(2)")]
    fn rejects_typmods_explicitly(#[case] input: &str) {
        let context = TestParseContext::default().with_dialect(Dialect::postgres());
        assert!(
            parse_client_type(input, &context)
                .unwrap_err()
                .to_string()
                .contains("type modifiers")
        );
    }

    #[rstest::rstest]
    #[case::empty("")]
    #[case::comments("/* comment */")]
    #[case::trailing("integer; SELECT 1")]
    #[case::semicolon("integer;")]
    #[case::two_types("integer, text")]
    #[case::expression("integer + 1")]
    #[case::setof("SETOF integer")]
    #[case::percent("public.kind%TYPE")]
    #[case::malformed_quote("\"kind")]
    #[case::empty_quote(r#""""#)]
    #[case::empty_schema(".int4")]
    #[case::empty_component("public..int4")]
    #[case::four_components("a.b.c.d")]
    #[case::keyword_first("select")]
    #[case::number_first("1kind")]
    #[case::backtick("`kind`")]
    #[case::bracket_quote("[kind]")]
    #[case::negative_bound("integer[-1]")]
    #[case::huge_bound("integer[2147483648]")]
    #[case::fractional_bound("integer[1.5]")]
    #[case::range_bound("integer[1:2]")]
    #[case::incomplete_array("integer[")]
    #[case::array_empty_bound("integer ARRAY[]")]
    #[case::array_two_bounds("integer ARRAY[2][3]")]
    #[case::array_suffix_mix("integer[] ARRAY")]
    #[case::float_zero("float(0)")]
    #[case::float_large("float(54)")]
    #[case::float_negative("float(-1)")]
    #[case::float_fraction("float(1.5)")]
    #[case::float_empty("float()")]
    #[case::unicode_incomplete(r#"U&"a\003""#)]
    #[case::unicode_bad_hex(r#"U&"a\zzzz""#)]
    #[case::unicode_zero(r#"U&"\0000""#)]
    #[case::unicode_high(r#"U&"\d800""#)]
    #[case::unicode_low(r#"U&"\dc00""#)]
    #[case::unicode_range(r#"U&"\+110000""#)]
    #[case::escape_non_ascii(r#"U&"inté0034" UESCAPE 'é'"#)]
    #[case::escape_hex(r#"U&"a" UESCAPE 'a'"#)]
    #[case::escape_empty(r#"U&"a" UESCAPE ''"#)]
    #[case::escape_long(r#"U&"a" UESCAPE '!!'"#)]
    #[case::escape_missing(r#"U&"a" UESCAPE"#)]
    #[case::escape_without_unicode(r#""a" UESCAPE '!'"#)]
    #[case::nul("int4\0text")]
    fn rejects_invalid_postgres_input(#[case] input: &str) {
        let context = TestParseContext::default().with_dialect(Dialect::postgres());
        assert!(
            parse_client_type(input, &context).is_err(),
            "accepted {input:?}"
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::plain("INTEGER", "INTEGER", false)]
    #[case::empty_bracket("[]", "", false)]
    #[case::multiword("DOUBLE PRECISION", "DOUBLE PRECISION", false)]
    #[case::backtick("`My Kind`", "My Kind", false)]
    #[case::bracket("[My Kind]", "My Kind", false)]
    #[case::single("'My Kind'", "My Kind", false)]
    #[case::double(r#""My Kind""#, "My Kind", false)]
    #[case::escaped_backtick("`a``b`", "a`b", false)]
    #[case::escaped_double(r#""a""b""#, "a\"b", false)]
    #[case::parts("[DOUBLE] /* comment */ `PRECISION`", "DOUBLE PRECISION", false)]
    #[case::array("kind[][]", "kind", true)]
    #[case::quoted_array("[kind][]", "kind", true)]
    #[case::literal_suffix("[kind[]", "kind[", false)]
    #[case::numeric_bracket_name("kind[2][]", "kind 2", true)]
    #[case::literal_parentheses("[numeric(5,2)]", "numeric(5,2)", false)]
    fn interprets_sqlite_declared_names(
        #[case] input: &str,
        #[case] name: &str,
        #[case] array: bool,
    ) {
        let context = TestParseContext::default().with_dialect(Dialect::sqlite());
        assert_eq!(
            parse_client_type(input, &context).unwrap(),
            TypeReference::concrete_type_ref(&FullName::from_local_name(name), array)
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::empty("")]
    #[case::size("DECIMAL(5,2)")]
    #[case::array_size("DECIMAL(5,2)[]")]
    #[case::unclosed_bracket("[kind")]
    #[case::unclosed_quote("`kind")]
    #[case::qualification("main.kind")]
    #[case::trailing("kind; SELECT 1")]
    #[case::expression("kind + 1")]
    fn rejects_invalid_sqlite_input(#[case] input: &str) {
        let context = TestParseContext::default().with_dialect(Dialect::sqlite());
        assert!(
            parse_client_type(input, &context).is_err(),
            "accepted {input:?}"
        );
    }

    #[test]
    fn canonical_parsing_keeps_its_separate_contract() {
        let canonical: TypeReference = "INTEGER".parse().unwrap();
        assert_eq!(
            canonical.full_name().expect("nominal type identity"),
            &FullName::from_local_name("INTEGER")
        );
        let encoded = ::serde_json::to_string(&canonical).unwrap();
        assert_eq!(
            ::serde_json::from_str::<TypeReference>(&encoded).unwrap(),
            canonical
        );
        assert_eq!(canonical.to_string(), r#""INTEGER""#);
    }

    #[rstest::rstest]
    #[case::ascii("a", 63)]
    #[case::multibyte("あ", 21)]
    fn applies_postgres_identifier_byte_limit(#[case] unit: &str, #[case] count: usize) {
        let context = TestParseContext::default().with_dialect(Dialect::postgres());
        let name = unit.repeat(count + 1);
        let expected = FullName::from_local_name(&unit.repeat(count));
        assert_eq!(
            parse_client_type(&format!("\"{name}\""), &context).unwrap(),
            TypeReference::from_full_name(expected)
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::postgres("postgres", "SQLite", "integer", "pg_catalog.int4")]
    #[case::sqlite("SQLite", "PostgreSQL", "integer", "integer")]
    fn uses_selected_dialect_independently_of_engine(
        #[case] dialect: &str,
        #[case] engine: &str,
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let metadata = ::serde_json::from_value(::serde_json::json!({
            "engine": engine, "dialect": dialect, "version": "17", "search_path": ["public"]
        }))
        .unwrap();
        let context = TestParseContext::default().with_metadata(&metadata);
        assert_eq!(
            parse_client_type(input, &context).unwrap(),
            expected.parse::<TypeReference>().unwrap()
        );
    }

    #[rstest::rstest]
    #[case::postgres(Dialect::postgres())]
    #[cfg_attr(feature = "sqlite", case::sqlite(Dialect::sqlite()))]
    fn expression_tails_are_syntax_errors(#[case] dialect: Dialect) {
        let context = TestParseContext::default().with_dialect(dialect);
        let error = parse_client_type("int4 + f()", &context).unwrap_err();
        assert!(!error.to_string().contains("type modifiers"));
    }
}

fn invalid(detail: &str) -> AnalysisError {
    AnalysisError::unexpected_input_error(&format!("SQL type name: {detail}"))
}

fn typmod() -> AnalysisError {
    invalid("type modifiers are not representable in client parameter types")
}
