use crate::{ExtensionConfigError, ExtensionVersion};
use std::path::Path;

// PostgreSQL 17: guc-file.l (ParseConfigFp/DeescapeQuotedString) and
// extension.c (parse_extension_control_file). Equals is optional; the last
// default_version wins. Only lexical validation and default selection belong
// here: this is not an extension installer or a general GUC/include processor.
pub(super) fn default_version(
    contents: &[u8],
    path: &Path,
) -> Result<Option<String>, ExtensionConfigError> {
    let mut version = None;
    let mut version_line = 0;
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
        if name == b"default_version" {
            version = Some(value);
            version_line = index + 1;
        }
    }
    let Some(value) = version else {
        return Ok(None);
    };
    let Ok(value) = String::from_utf8(value) else {
        return ExtensionConfigError::invalid_control_file(
            path,
            version_line,
            "default_version is not valid UTF-8",
        );
    };
    if ExtensionVersion::installation(&value).is_err() {
        return ExtensionConfigError::invalid_control_file(
            path,
            version_line,
            "default_version must be a nonempty installation version without path separators, --, or a leading/trailing dash",
        );
    }
    Ok(Some(value))
}

#[cfg(test)]
mod test_default_version {
    use super::default_version;
    use std::path::Path;

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
            &b"\xff='ignored'\ndefault_version='1.1'"[..],
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
    use super::ControlLine;
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
