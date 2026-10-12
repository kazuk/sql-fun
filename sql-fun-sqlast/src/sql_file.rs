use std::path::Path;

/// Read and hash original text before excluding psql commands from SQL analysis.
pub(super) fn read_sql_file(
    path: &Path,
    content_hash: &mut Option<String>,
) -> Result<String, std::io::Error> {
    *content_hash = None;
    let text = std::fs::read_to_string(path)?;
    let mut hasher = ::sql_fun_core::Hasher::new();
    hasher.append_str(&text);
    *content_hash = Some(hasher.finish());
    Ok(comment_psql_commands(text))
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
    #[rstest::rstest]
    #[case::normal("\\restrict abc\nSELECT 1;\n", "-- strict abc\nSELECT 1;\n")]
    #[case::indented("  \\x\r\nSELECT 1;", "  --\r\nSELECT 1;")]
    #[case::short("\\", " ")]
    #[case::unicode("\\é\n", "-- \n")]
    fn preserves_command_offsets(#[case] input: &str, #[case] expected: &str) {
        let actual = super::comment_psql_commands(input.to_owned());
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), input.len());
    }
}
