use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

/// An iterator over the lines of a file, preserving line endings.
///
/// # Overview
/// This iterator reads from a file line-by-line using
/// [`std::io::BufRead::read_line`], **keeping the trailing `\n` (if present)**
/// in each returned `String`.
///
/// This differs from [`std::io::BufRead::lines`], which strips line endings.
/// Preserving `\n` is intentional — it allows callers to `push_str` the lines
/// directly into an output `String` without re-inserting line breaks.
///
/// # Common Pitfall
/// Do **not** treat the returned `String` as if it came from `.lines()`,
/// or you may end up with duplicated or missing newlines in the output.
///
/// ```ignore
/// // Correct: preserves original line breaks
/// let mut out = String::new();
/// for line in file_lines("input.sql")? {
///     let line = line?;
///     out.push_str(&line); // `line` already has `\n`
/// }
///
/// // Incorrect: adding your own `\n` will double the newlines
/// for line in file_lines("input.sql")? {
///     let line = line?;
///     out.push_str(&line);
///     out.push('\n'); // ❌ This will produce two newlines
/// }
/// ```
///
/// # Error Handling
/// Each item is `Result<String, std::io::Error>`.
/// The iterator yields an error if reading the underlying file fails.
///
/// # See Also
/// - [`std::io::BufRead::read_line`] — underlying API used here.
/// - [`std::io::BufRead::lines`] — strips newlines, unlike this iterator.
fn file_lines<P: AsRef<Path>>(path: P) -> Result<LineWithLfIter, std::io::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(LineWithLfIter { reader, eof: false })
}

struct LineWithLfIter {
    reader: BufReader<File>,
    eof: bool,
}

impl Iterator for LineWithLfIter {
    type Item = Result<String, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.eof {
            return None;
        }

        let mut buf = String::new();
        match self.reader.read_line(&mut buf) {
            Ok(read) => {
                if read == 0 {
                    self.eof = true;
                }
                Some(Ok(buf))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

fn comment_psql_meta_command(line: String) -> String {
    if line.trim_start().starts_with('\\') {
        format!("-- {line}")
    } else {
        line
    }
}

pub(super) fn read_sql_file<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    let mut result = String::new();
    for line_res in file_lines(path)? {
        let line = comment_psql_meta_command(line_res?);
        result.push_str(&line);
    }
    Ok(result)
}
