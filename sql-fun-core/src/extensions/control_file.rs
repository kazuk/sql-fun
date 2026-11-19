use ::combine::{
    Parser, attempt, between, choice, eof, many, many1,
    parser::char::{char, string},
    satisfy, sep_by, skip_many,
};

use crate::{ExtensionConfigError, ExtensionVersion};
use std::path::{Path, PathBuf};

/// Control-file settings used to preprocess an extension installation or update script.
#[derive(Clone, Debug, Default)]
pub struct ExtensionScriptSettings {
    schema: Option<String>,
    relocatable: bool,
    requires: Vec<String>,
    default_version: Option<ExtensionVersion>,
    directory: Option<PathBuf>,
    encoding: Option<String>,
    module_pathname: Option<String>,
}

impl ExtensionScriptSettings {
    /// Fixed installation schema, when declared.
    #[must_use]
    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    /// Whether the extension permits relocation after installation.
    #[must_use]
    pub const fn relocatable(&self) -> bool {
        self.relocatable
    }

    /// Required extension names in control-file order.
    #[must_use]
    pub fn requires(&self) -> &[String] {
        &self.requires
    }

    /// Literal replacement for `MODULE_PATHNAME`, when declared.
    #[must_use]
    pub fn module_pathname(&self) -> Option<&str> {
        self.module_pathname.as_deref()
    }

    /// Declared SQL script encoding.
    #[must_use]
    pub fn encoding(&self) -> Option<&str> {
        self.encoding.as_deref()
    }

    pub(super) const fn default_version(&self) -> Option<&ExtensionVersion> {
        self.default_version.as_ref()
    }

    pub(super) fn script_directory(&self, control_directory: &Path) -> PathBuf {
        // Relative paths are anchored in the installation's share directory.
        match &self.directory {
            Some(path) => control_directory
                .parent()
                .unwrap_or(control_directory)
                .join(path),
            None => control_directory.to_path_buf(),
        }
    }

    pub(super) fn apply_file(
        &mut self,
        path: &Path,
        auxiliary: bool,
    ) -> Result<(), ExtensionConfigError> {
        let contents = match std::fs::read(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        self.apply(&contents, path, auxiliary)
    }

    fn apply(
        &mut self,
        contents: &[u8],
        path: &Path,
        auxiliary: bool,
    ) -> Result<(), ExtensionConfigError> {
        // The final default_version assignment wins.
        let mut default_version = None;
        visit_assignments(contents, path, |name, value, line| {
            match name {
                b"directory" | b"default_version" if auxiliary => {
                    return ExtensionConfigError::invalid_control_file(
                        path,
                        line,
                        "directory and default_version are primary control settings",
                    );
                }
                b"default_version" => {
                    default_version = Some((value, line));
                    return Ok(());
                }
                b"schema" | b"relocatable" | b"requires" | b"module_pathname" | b"directory"
                | b"encoding" => {}
                b"comment" | b"superuser" | b"trusted" | b"no_relocate" => return Ok(()),
                _ => {
                    return ExtensionConfigError::invalid_control_file(
                        path,
                        line,
                        "unrecognized extension control setting",
                    );
                }
            }
            let Ok(value) = String::from_utf8(value) else {
                return ExtensionConfigError::invalid_control_file(
                    path,
                    line,
                    "invalid UTF-8 in script setting",
                );
            };
            match name {
                b"schema" => self.schema = Some(value),
                b"requires" => {
                    self.requires = match required_extensions(&value) {
                        Ok(names) => names,
                        Err(reason) => {
                            return ExtensionConfigError::invalid_control_file(path, line, reason);
                        }
                    };
                }
                b"encoding" => self.encoding = Some(value),
                b"directory" => self.directory = Some(value.into()),
                b"module_pathname" => self.module_pathname = Some(value),
                _ => {
                    self.relocatable = match value.to_ascii_lowercase().as_str() {
                        "1" | "t" | "tr" | "tru" | "true" | "y" | "ye" | "yes" | "on" => true,
                        "0" | "f" | "fa" | "fal" | "fals" | "false" | "n" | "no" | "of" | "off" => {
                            false
                        }
                        _ => {
                            return ExtensionConfigError::invalid_control_file(
                                path,
                                line,
                                "invalid relocatable boolean",
                            );
                        }
                    };
                }
            }
            Ok(())
        })?;
        if let Some((value, line)) = default_version {
            self.default_version = Some(parse_default_version(value, path, line)?);
        }
        if self.relocatable && self.schema.is_some() {
            return ExtensionConfigError::invalid_control_file(
                path,
                1,
                "relocatable extension cannot specify a schema",
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_default_version {
    use crate::{ExtensionConfigError, ExtensionScriptSettings};
    use std::path::Path;

    fn default_version(
        contents: &[u8],
        path: &Path,
    ) -> Result<Option<String>, ExtensionConfigError> {
        let mut control = ExtensionScriptSettings::default();
        control.apply(contents, path, false)?;
        Ok(control.default_version.map(|version| version.to_string()))
    }

    #[test]
    fn follows_postgres_assignments_and_last_value() {
        let input = "# comment\ncomment='ignore # and ''quotes'' here'\ndefault_version = 'old'\n\tdefault_version 'release_2' # last\r\n";
        assert_eq!(
            default_version(input.as_bytes(), Path::new("ext.control"))
                .unwrap()
                .as_deref(),
            Some("release_2")
        );
        assert_eq!(
            default_version(b"comment = 'default_version = 9'", Path::new("ext.control")).unwrap(),
            None
        );
        assert_eq!(
            default_version(br"comment='\377'", Path::new("ext.control")).unwrap(),
            None
        );
    }

    #[test]
    fn default_errors_report_the_source_line_and_validate_only_the_last_value() {
        let path = Path::new("ext.control");
        assert!(
            default_version(b"# comment\ndefault_version=''", path)
                .unwrap_err()
                .to_string()
                .contains("ext.control:2")
        );
        assert_eq!(
            default_version(b"default_version=''\ndefault_version='2'", path)
                .unwrap()
                .as_deref(),
            Some("2")
        );
        assert_eq!(
            default_version(br"default_version='\061.\061'", path)
                .unwrap()
                .as_deref(),
            Some("1.1")
        );
    }

    #[test]
    fn decodes_only_the_final_default_version() {
        let path = Path::new("ext.control");
        assert!(
            default_version(br"default_version='\377'", path)
                .unwrap_err()
                .to_string()
                .contains("not valid UTF-8")
        );
        assert_eq!(
            default_version(b"default_version='\\377'\ndefault_version='2'", path)
                .unwrap()
                .as_deref(),
            Some("2")
        );
    }

    #[test]
    fn ignores_non_utf8_bytes_outside_the_selected_version() {
        let path = Path::new("ext.control");
        for input in [
            &b"# \xff\ndefault_version='1.1'"[..],
            &b"comment='\xff'\ndefault_version='1.1'"[..],
            &b"comment=\xff\ndefault_version='1.1'"[..],
            &b"default_version='\xff'\ndefault_version='1.1'"[..],
        ] {
            assert_eq!(
                default_version(input, path).unwrap().as_deref(),
                Some("1.1")
            );
        }
        assert_eq!(
            default_version(b"# \xff\ncomment='\xff'", path).unwrap(),
            None
        );
        let error = default_version(b"# comment\ndefault_version='\xff'", path).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("ext.control:2: default_version is not valid UTF-8")
        );
    }

    #[test]
    fn reports_unsupported_include_directives_instead_of_inferring() {
        for directive in ["include", "include_if_exists", "INCLUDE_DIR"] {
            let input = format!("{directive}='other.control'");
            assert!(
                default_version(input.as_bytes(), Path::new("ext.control"))
                    .unwrap_err()
                    .to_string()
                    .contains("include directives are not supported")
            );
        }
    }

    #[test]
    fn rejects_malformed_lines_even_for_unrelated_parameters() {
        for input in [
            "default_version=",
            "default_version='unclosed",
            "comment='a' 'b'",
            "default_version=1.2.3",
            "=1",
            "comment='a\nb'",
            "default_version=\"1\"",
            "comment='a\\",
            "a.b.c=1",
            "comment=@",
            "default_version=1e2",
            "default_version=release.beta",
        ] {
            let error = default_version(input.as_bytes(), Path::new("ext.control")).unwrap_err();
            assert!(
                error.to_string().contains("ext.control:1"),
                "{input}: {error}"
            );
        }
    }
}

type ControlAssignment<'a> = (&'a [u8], Vec<u8>);

struct ControlLine<'a> {
    rest: &'a [u8],
}

impl<'a> ControlLine<'a> {
    fn parse(line: &'a [u8]) -> Result<Option<ControlAssignment<'a>>, &'static str> {
        let mut cursor = Self { rest: line };
        cursor.whitespace();
        if cursor.finished() {
            return Ok(None);
        }
        let name = cursor.word();
        if !Self::identifier(name) && !Self::qualified_identifier(name) {
            return Err("expected parameter name");
        }
        cursor.whitespace();
        if cursor.rest.first() == Some(&b'=') {
            cursor.rest = &cursor.rest[1..];
            cursor.whitespace();
        }
        let value = if cursor.rest.first() == Some(&b'\'') {
            cursor.quoted()?
        } else {
            let token = cursor.word();
            // GUC_QUALIFIED_ID wins lexer ties over UNQUOTED_STRING, but
            // ParseConfigFp does not accept it as a parameter value.
            if Self::qualified_identifier(token)
                || (!Self::unquoted(token) && !Self::numeric(token))
            {
                return Err("expected a single parameter value");
            }
            token.to_vec()
        };
        cursor.whitespace();
        if !cursor.finished() {
            return Err("unexpected token after parameter value");
        }
        // PostgreSQL's deescaped values are C strings.
        let mut value = value;
        value.truncate(Self::prefix(&value, |byte| *byte != 0));
        Ok(Some((name, value)))
    }

    fn whitespace(&mut self) {
        while matches!(self.rest.first(), Some(b' ' | b'\t' | b'\r')) {
            self.rest = &self.rest[1..];
        }
    }

    fn finished(&self) -> bool {
        self.rest.is_empty() || self.rest.first() == Some(&b'#')
    }

    fn word(&mut self) -> &'a [u8] {
        let end = self
            .rest
            .iter()
            .position(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'=' | b'#' | b'\''))
            .unwrap_or(self.rest.len());
        let word = &self.rest[..end];
        self.rest = &self.rest[end..];
        word
    }

    fn quoted(&mut self) -> Result<Vec<u8>, &'static str> {
        self.rest = &self.rest[1..];
        let mut value = Vec::new();
        while let Some((&byte, tail)) = self.rest.split_first() {
            self.rest = tail;
            match byte {
                b'\'' => {
                    if self.rest.first() != Some(&b'\'') {
                        return Ok(value);
                    }
                    self.rest = &self.rest[1..];
                    value.push(b'\'');
                }
                b'\\' => {
                    let Some((&escaped, tail)) = self.rest.split_first() else {
                        return Err("unfinished string escape");
                    };
                    self.rest = tail;
                    if (b'0'..=b'7').contains(&escaped) {
                        let mut octal = escaped - b'0';
                        for _ in 0..2 {
                            let Some((&digit, tail)) = self.rest.split_first() else {
                                break;
                            };
                            if !(b'0'..=b'7').contains(&digit) {
                                break;
                            }
                            octal = octal.wrapping_mul(8).wrapping_add(digit - b'0');
                            self.rest = tail;
                        }
                        value.push(octal);
                    } else {
                        value.push(match escaped {
                            b'b' => 8,
                            b'f' => 12,
                            b'n' => b'\n',
                            b'r' => b'\r',
                            b't' => b'\t',
                            other => other,
                        });
                    }
                }
                other => value.push(other),
            }
        }
        Err("unterminated quoted string")
    }

    const fn letter(byte: u8) -> bool {
        byte.is_ascii_alphabetic() || byte == b'_' || byte >= 128
    }

    fn identifier(word: &[u8]) -> bool {
        let Some((&first, tail)) = word.split_first() else {
            return false;
        };
        if !Self::letter(first) {
            return false;
        }
        for byte in tail {
            if !Self::letter(*byte) && !byte.is_ascii_digit() {
                return false;
            }
        }
        true
    }

    fn qualified_identifier(word: &[u8]) -> bool {
        let mut parts = word.split(|byte| *byte == b'.');
        Self::identifier(parts.next().unwrap_or_default())
            && Self::identifier(parts.next().unwrap_or_default())
            && parts.next().is_none()
    }

    fn unquoted(word: &[u8]) -> bool {
        let Some((&first, tail)) = word.split_first() else {
            return false;
        };
        if !Self::letter(first) {
            return false;
        }
        for byte in tail {
            if !Self::letter(*byte) && !byte.is_ascii_digit() && !b"-._:/".contains(byte) {
                return false;
            }
        }
        true
    }

    fn prefix(word: &[u8], predicate: fn(&u8) -> bool) -> usize {
        let mut length = 0;
        for byte in word {
            if !predicate(byte) {
                break;
            }
            length += 1;
        }
        length
    }

    fn all(word: &[u8], predicate: fn(&u8) -> bool) -> bool {
        Self::prefix(word, predicate) == word.len()
    }

    fn numeric(word: &[u8]) -> bool {
        let unsigned = if matches!(word.first(), Some(b'+' | b'-')) {
            &word[1..]
        } else {
            word
        };
        if unsigned.is_empty() {
            return false;
        }
        let count = Self::prefix(unsigned, u8::is_ascii_digit);
        let mut digits = &unsigned[count..];
        if count > 0 && Self::all(digits, u8::is_ascii_alphabetic) {
            return true;
        }
        if let Some(hex) = unsigned.strip_prefix(b"0x") {
            let count = Self::prefix(hex, u8::is_ascii_hexdigit);
            return count > 0 && Self::all(&hex[count..], u8::is_ascii_alphabetic);
        }
        if digits.first() != Some(&b'.') {
            return false;
        }
        digits = &digits[1..];
        digits = &digits[Self::prefix(digits, u8::is_ascii_digit)..];
        if digits.is_empty() {
            return true;
        }
        if !matches!(digits.first(), Some(b'e' | b'E')) {
            return false;
        }
        digits = &digits[1..];
        if matches!(digits.first(), Some(b'+' | b'-')) {
            digits = &digits[1..];
        }
        !digits.is_empty() && Self::all(digits, u8::is_ascii_digit)
    }
}

#[cfg(test)]
mod test_control_line {
    use crate::extensions::control_file::ControlLine;
    #[test]
    fn deescapes_postgres_strings() {
        let input = r"default_version='a''b\'c\141\q\t\b\f\n\r\\'";
        assert_eq!(
            ControlLine::parse(input.as_bytes()).unwrap().unwrap().1,
            "a'b'caq\t\u{8}\u{c}\n\r\\".as_bytes()
        );
    }

    #[test]
    fn accepts_postgres_unquoted_tokens() {
        for value in [
            "1.1",
            "-1",
            "+1.2e-3",
            "0xFF",
            "0x",
            "0xg",
            "123ms",
            "release-2",
            ".",
            "1.",
            "foo/bar",
        ] {
            let input = format!("default_version={value}");
            assert_eq!(
                ControlLine::parse(input.as_bytes()).unwrap().unwrap().1,
                value.as_bytes()
            );
        }
    }
}

fn parse_default_version(
    value: Vec<u8>,
    path: &Path,
    line: usize,
) -> Result<ExtensionVersion, ExtensionConfigError> {
    let Ok(value) = String::from_utf8(value) else {
        return ExtensionConfigError::invalid_control_file(
            path,
            line,
            "default_version is not valid UTF-8",
        );
    };
    ExtensionVersion::installation(&value).or_else(|_| ExtensionConfigError::invalid_control_file(
        path, line, "default_version must be a nonempty installation version without path separators, --, or a leading/trailing dash",
    ))
}

fn required_extensions(input: &str) -> Result<Vec<String>, &'static str> {
    let whitespace = || skip_many(satisfy(|ch: char| ch.is_ascii_whitespace()));
    let quoted = between(
        char('"'),
        char('"'),
        many(choice((
            attempt(string("\"\"")).map(|_| '"'),
            satisfy(|ch| ch != '"'),
        ))),
    );
    let unquoted =
        many1::<String, _, _>(satisfy(|ch: char| ch != ',' && !ch.is_ascii_whitespace()))
            .map(|name| name.to_ascii_lowercase());
    let identifier = choice((quoted, unquoted)).skip(whitespace());
    let separator = char(',').skip(whitespace());

    whitespace()
        .with(sep_by(identifier, separator))
        .skip(eof())
        .parse(input)
        .map(|(names, _)| names)
        .map_err(|_| "requires must be a comma-separated list of extension names")
}

#[cfg(test)]
mod test_required_extensions {
    use crate::extensions::control_file::required_extensions;

    #[test]
    fn parses_quotes_escapes_and_whitespace() {
        let names =
            required_extensions(" \tCuBe, \"With space, comma and \"\"quotes\"\"\",\r\nother ")
                .unwrap();
        assert_eq!(
            names,
            ["cube", r#"With space, comma and "quotes""#, "other"]
        );
        assert_eq!(required_extensions(" \t").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn rejects_malformed_identifier_lists() {
        for input in [
            ",cube",
            "cube,",
            "cube,,other",
            "cube other",
            r#""unterminated"#,
        ] {
            assert!(required_extensions(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn preserves_identifiers_longer_than_the_standard_postgresql_limit() {
        let unquoted = "A".repeat(70);
        let quoted = "É".repeat(40);
        let names = required_extensions(&format!(r#"{unquoted}, "{quoted}""#)).unwrap();
        assert_eq!(names, [unquoted.to_ascii_lowercase(), quoted]);
    }
}

fn visit_assignments(
    contents: &[u8],
    path: &Path,
    mut visit: impl FnMut(&[u8], Vec<u8>, usize) -> Result<(), ExtensionConfigError>,
) -> Result<(), ExtensionConfigError> {
    for (index, line) in contents.split(|byte| *byte == b'\n').enumerate() {
        let parsed = ControlLine::parse(line);
        let (name, value) = match parsed {
            Ok(Some(pair)) => pair,
            Ok(None) => continue,
            Err(reason) => {
                return ExtensionConfigError::invalid_control_file(path, index + 1, reason);
            }
        };
        if name.eq_ignore_ascii_case(b"include")
            || name.eq_ignore_ascii_case(b"include_if_exists")
            || name.eq_ignore_ascii_case(b"include_dir")
        {
            return ExtensionConfigError::invalid_control_file(
                path,
                index + 1,
                "control-file include directives are not supported",
            );
        }
        visit(name, value, index + 1)?;
    }
    Ok(())
}
