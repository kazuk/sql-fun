use std::path::{Path, PathBuf};

/// Immutable SQL file snapshot with the hash of its original UTF-8 contents.
/// Reading and hashing happen once; subsequent analysis never reopens the file.
#[derive(Debug)]
pub struct SqlFileSnapshot {
    path: PathBuf,
    text: String,
    hash: String,
}

impl SqlFileSnapshot {
    /// Read the explicitly supplied file and hash exactly the bytes read.
    ///
    /// # Errors
    /// Returns file access or UTF-8 decoding errors.
    pub fn read(path: &Path) -> Result<Self, std::io::Error> {
        let text = std::fs::read_to_string(path)?;
        let mut hasher = ::sql_fun_core::Hasher::new();
        hasher.append_str(&text);
        Ok(Self {
            path: path.to_path_buf(),
            text,
            hash: hasher.finish(),
        })
    }

    /// Original-content hash, before dialect preprocessing.
    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.hash
    }

    /// Source path retained for analysis diagnostics and cache identity.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    #[cfg(feature = "sqlite")]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }
    pub(crate) fn postgres_text(&self) -> String {
        comment_psql_commands(self.text.clone())
    }
}

fn comment_psql_commands(mut text: String) -> String {
    // Preserve UTF-8 validity and all original byte offsets.
    let mut offset = 0;
    let replacements: Vec<_> = text
        .split_inclusive('\n')
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let start = offset + line.len() - trimmed.len();
            offset += line.len();
            if trimmed.starts_with('\\') {
                Some(start)
            } else {
                None
            }
        })
        .collect();
    for start in replacements {
        // A short command must not consume the newline or a multibyte character.
        let end = text[start..]
            .chars()
            .take(3)
            .take_while(|c| c.is_ascii() && *c != '\n' && *c != '\r')
            .count();
        if end >= 2 {
            let replacement = if end == 2 { "--" } else { "-- " };
            text.replace_range(start..start + end, replacement);
        } else {
            let line_end = text[start..]
                .find(['\r', '\n'])
                .map_or(text.len(), |len| start + len);
            let length = line_end - start;
            let replacement = if length >= 2 {
                format!("--{}", " ".repeat(length - 2))
            } else {
                " ".to_owned()
            };
            text.replace_range(start..line_end, &replacement);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use crate::sql_file::comment_psql_commands;

    #[rstest::rstest]
    #[case::normal("\\restrict abc\nSELECT 1;\n", "-- strict abc\nSELECT 1;\n")]
    #[case::indented("  \\x\r\nSELECT 1;", "  --\r\nSELECT 1;")]
    #[case::short("\\", " ")]
    #[case::unicode("\\é\n", "-- \n")]
    fn preserves_command_offsets(#[case] input: &str, #[case] expected: &str) {
        let actual = comment_psql_commands(input.to_owned());
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), input.len());
    }
}
