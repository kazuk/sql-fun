//! postgres internal .dat file parser

use chumsky::error::Rich;
use chumsky::prelude::*;
use clap::Parser as ClapParser;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

/// Convert byte offset in source to (line, column).
fn pos_to_line_col(source: &str, pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.chars().enumerate() {
        if i == pos {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// CLI argument for `parse_dat`
#[derive(ClapParser)]
pub struct ParseDatArgs {
    /// Input .dat file path
    input_file: PathBuf,
    /// Optional output JSON file path
    output_file: Option<PathBuf>,
}

/// parse errors
#[derive(Error, Debug)]
pub enum ParseDatError {
    /// IO
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// serialize
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// parse string
    #[error("Parse error:\n{0}")]
    Parse(String),
}

impl ParseDatArgs {
    /// Parse the input file and optionally write JSON output.
    ///
    /// # Errors
    ///
    /// Returns [`ParseDatError`] when reading, parsing, or writing the data fails.
    pub fn run(&self) -> Result<(), ParseDatError> {
        // parse and collect detailed errors
        let content = fs::read_to_string(&self.input_file)?;
        // parse and collect detailed errors
        let parser = dat_parser();
        let result = parser.parse(&content);
        if result.has_errors() {
            let (_ast, parse_errors) = result.into_output_errors();
            for err in parse_errors {
                let (line, col) = pos_to_line_col(&content, err.span().start);
                eprintln!(
                    "parse error at {}:{line} at column {col}",
                    self.input_file.display()
                );

                eprintln!("Error: {err:?}");
                let reason = err.reason();
                eprintln!("Reason: {reason:?}");
                for context in err.contexts() {
                    eprintln!("Context: {context:?}");
                }
            }
            return Err(ParseDatError::Parse(format!(
                "{} parsing failed",
                self.input_file.display()
            )));
        }
        let Some(value) = result.output() else {
            return Err(ParseDatError::Parse(String::from(
                "parser produced no output",
            )));
        };
        let output = serde_json::to_string_pretty(&value)?;
        if let Some(outfile) = &self.output_file {
            fs::write(outfile, output)?;
        } else {
            print!("{output}");
        }
        Ok(())
    }
}

fn ws<'a>() -> impl Parser<'a, &'a str, (), extra::Err<Rich<'a, char>>> {
    let comment = just('#').ignore_then(none_of('\n').repeated()).ignored();
    let space = any().filter(|c: &char| c.is_whitespace()).to(());
    comment.or(space).repeated().ignored()
}

fn just_ws<'a>(expect: char) -> impl Parser<'a, &'a str, char, extra::Err<Rich<'a, char>>> {
    ws().ignore_then(just(expect)).then_ignore(ws())
}

fn just_ws_str(expect: &str) -> impl Parser<'_, &str, &str, extra::Err<Rich<'_, char>>> {
    ws().ignore_then(just(expect).labelled("expect"))
        .then_ignore(ws())
}

fn parse_string<'a>() -> impl Parser<'a, &'a str, Value, extra::Err<Rich<'a, char>>> {
    ws().ignore_then(
        just('\'')
            .ignore_then(
                just('\\')
                    .ignore_then(any())
                    .map(|c| c)
                    .or(any().filter(|c: &char| *c != '\\' && *c != '\''))
                    .repeated()
                    .collect(),
            )
            .then_ignore(just('\'').then_ignore(ws()))
            .map(Value::String),
    )
}

fn parse_null<'a>() -> impl Parser<'a, &'a str, Value, extra::Err<Rich<'a, char>>> {
    just_ws('-').map(|_| Value::Null)
}

fn parse_value<'a>() -> impl Parser<'a, &'a str, Value, extra::Err<Rich<'a, char>>> {
    ws().ignore_then(parse_string().or(parse_null()).labelled("string or null"))
        .then_ignore(ws())
}

fn parse_pair<'a>() -> impl Parser<'a, &'a str, (String, Value), extra::Err<Rich<'a, char>>> {
    let key = chumsky::text::ascii::ident().labelled("key");
    ws().ignore_then(
        key.then_ignore(just_ws_str("=>"))
            .then(parse_value())
            .map(|(k, v)| (k.to_string(), v)),
    )
    .map_err(|e| Rich::custom(*e.span(), "pair parse error"))
}

fn parse_record<'a>() -> impl Parser<'a, &'a str, serde_json::Value, extra::Err<Rich<'a, char>>> {
    just_ws('{')
        .ignore_then(
            parse_pair()
                .separated_by(just_ws(','))
                .allow_trailing()
                .collect()
                .labelled("pairs"),
        )
        .then_ignore(just_ws('}'))
        .map(|v: HashMap<String, Value>| serde_json::Value::Object(serde_json::Map::from_iter(v)))
}

fn dat_parser<'a>() -> impl Parser<'a, &'a str, serde_json::Value, extra::Err<Rich<'a, char>>> {
    just_ws('[')
        .ignore_then(
            parse_record()
                .separated_by(just_ws(','))
                .allow_trailing()
                .collect()
                .labelled("elements"),
        )
        .then_ignore(just_ws(']'))
        .map(serde_json::Value::Array)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::Value;
    use testresult::TestResult;

    const WRAPPERS: &[(&str, &str)] = &[
        ("", ""),
        (" ", " "),
        ("\n", "\n"),
        ("# comment\n", ""),
        ("", "# comment\n"),
        ("\n# comment\n", "\n"),
    ];

    #[rstest(input, expected,
        case("'hello'", Value::String("hello".to_string())),
        case("'a'",     Value::String("a".to_string())),
        case("-",       Value::Null),
    )]
    fn test_parse_value(input: &str, expected: Value) -> TestResult {
        for (pre, suf) in WRAPPERS {
            let s = format!("{}{}{}", pre, input, suf);
            let v = parse_value().parse(s.as_str()).unwrap();
            assert_eq!(v, expected, "failed on {:?}", s);
        }
        Ok(())
    }

    #[rstest(input, exp_key, exp_val,
        case("key=>'value'", "key", Value::String("value".to_string())),
    )]
    fn test_parse_pair(input: &str, exp_key: &str, exp_val: Value) -> TestResult {
        for (pre, suf) in WRAPPERS {
            let s = format!("{}{}{}", pre, input, suf);
            let (key, val) = parse_pair().parse(s.as_str()).unwrap();
            assert_eq!(key, exp_key.to_string(), "failed on {:?}", s);
            assert_eq!(val, exp_val, "failed on {:?}", s);
        }
        Ok(())
    }

    #[rstest(input, exp_pairs,
        case("{a=>'b',c=>-}", vec![("a", Value::String("b".to_string())), ("c", Value::Null)]),
    )]
    fn test_parse_record(input: &str, exp_pairs: Vec<(&str, Value)>) -> TestResult {
        for (pre, suf) in WRAPPERS {
            let s = format!("{}{}{}", pre, input, suf);
            let rec = parse_record().parse(s.as_str()).unwrap();
            if let Value::Object(map) = rec {
                for (key, expected_val) in &exp_pairs {
                    assert_eq!(map.get(*key), Some(expected_val), "failed on {:?}", s);
                }
            } else {
                panic!("Expected object on {:?}", s);
            }
        }
        Ok(())
    }

    #[rstest(
        input,
        expected_len,
        case("[{a=>'b'},{c=>'d'}]", 2),
        case(
            r#"
# comment line
[
    {a=>'b'}
]
"#,
            1
        )
    )]
    fn test_parse_list(input: &str, expected_len: usize) -> TestResult {
        for (pre, suf) in WRAPPERS {
            let s = format!("{}{}{}", pre, input, suf);
            let arr = dat_parser().parse(s.as_str()).unwrap();
            if let Value::Array(items) = arr {
                assert_eq!(items.len(), expected_len, "failed on {:?}", s);
            } else {
                panic!("Expected array on {:?}", s);
            }
        }
        Ok(())
    }
}
