use crate::highlighter_theme::{HighlighterTheme, HighlighterThemeError};
use std::ops::Range;

use ::miette::highlighters::{Highlighter, HighlighterState};
use ::owo_colors::{Style, Styled};
use ::sql_fun_core::TokenStream;

pub(super) struct SqlHighlighter {
    tokens: Vec<StyledToken>,
}

struct StyledToken {
    span: Range<usize>,
    style: Style,
}

impl SqlHighlighter {
    pub(super) fn from_theme(
        theme: &HighlighterTheme,
        stream: &TokenStream,
    ) -> Result<Self, HighlighterThemeError> {
        let identifier = theme.identifier().style()?;
        let constant = theme.constant().style()?;
        let operator = theme.operator().style()?;
        let keyword = theme.keyword().style()?;
        let comment = theme.comment().style()?;
        let delimiter = theme.delimiter().style()?;
        let mut tokens = Vec::new();
        for token in stream.tokens() {
            let kind = token.kind();
            let style = if kind.is_identifier() {
                identifier
            } else if kind.is_constant() {
                constant
            } else if kind.is_operator() {
                operator
            } else if kind.is_keyword() {
                keyword
            } else if kind.is_comment() {
                comment
            } else {
                delimiter
            };
            tokens.push(StyledToken {
                span: token.span(),
                style,
            });
        }
        Ok(Self { tokens })
    }
}

impl Highlighter for SqlHighlighter {
    fn start_highlighter_state<'h>(
        &'h self,
        source: &dyn ::miette::SpanContents<'_>,
    ) -> Box<dyn HighlighterState + 'h> {
        Box::new(SqlHighlighterState {
            highlighter: self,
            source: str::from_utf8(source.data())
                .expect("retained SQL source is UTF-8")
                .to_owned(),
            offset: source.span().offset(),
            consumed: 0,
        })
    }
}

struct SqlHighlighterState<'h> {
    highlighter: &'h SqlHighlighter,
    source: String,
    offset: usize,
    consumed: usize,
}

impl HighlighterState for SqlHighlighterState<'_> {
    fn highlight_line<'s>(&mut self, line: &'s str) -> Vec<Styled<&'s str>> {
        let relative_start = self.source[self.consumed..]
            .find(line)
            .expect("miette line belongs to its retained SQL snippet");
        let local_start = self.consumed + relative_start;
        let start = self.offset + local_start;
        let end = start + line.len();
        self.consumed = local_start + line.len();
        let mut styled = Vec::new();
        let mut cursor = start;
        for token in &self.highlighter.tokens {
            let token_start = token.span.start.max(start);
            let token_end = token.span.end.min(end);
            if token_start >= token_end {
                continue;
            }
            if cursor < token_start {
                styled.push(Style::default().style(&line[cursor - start..token_start - start]));
            }
            styled.push(
                token
                    .style
                    .style(&line[token_start - start..token_end - start]),
            );
            cursor = token_end;
        }
        if cursor < end {
            styled.push(Style::default().style(&line[cursor - start..]));
        }
        styled
    }
}

#[cfg(test)]
mod test_highlighting {
    use super::SqlHighlighter;
    use crate::highlighter_theme::HighlighterTheme;
    use ::miette::SourceCode;
    use ::miette::highlighters::Highlighter;
    use ::sql_fun_core::SqlDialect;

    #[test]
    fn preserves_complete_lines_with_utf8_comments_and_trailing_whitespace()
    -> testresult::TestResult {
        let source = "select '日本語', 1;  -- comment\n  select 2;  \n";
        let tokens = ::sql_fun_sqlast::tokenize_sql(source, SqlDialect::PostgreSQL)?;
        let highlighter = SqlHighlighter::from_theme(&HighlighterTheme::default(), &tokens)?;
        let contents = source.read_span(&(0, source.len()).into(), 0, 0)?;
        let mut state = highlighter.start_highlighter_state(&*contents);
        for line in source.lines() {
            let actual: String = state
                .highlight_line(line)
                .into_iter()
                .map(|part| part.to_string())
                .collect();
            assert_eq!(actual, line);
        }
        Ok(())
    }

    #[test]
    fn clips_existing_multiline_tokens_for_snippet_highlighting() -> testresult::TestResult {
        let source = "select 1;\n/* first\n日本語 middle\nlast */ select 2;";
        let tokens = ::sql_fun_sqlast::tokenize_sql(source, SqlDialect::PostgreSQL)?;
        let theme = HighlighterTheme::from_toml_str(r#"comment = { foreground = "green" }"#)?;
        let highlighter = SqlHighlighter::from_theme(&theme, &tokens)?;
        let start = source.find("日本語").unwrap();
        let line = "日本語 middle";
        let contents = source.read_span(&(start, line.len()).into(), 0, 0)?;
        let mut state = highlighter.start_highlighter_state(&*contents);
        let actual: String = state
            .highlight_line(line)
            .into_iter()
            .map(|part| part.to_string())
            .collect();
        assert!(actual.contains(line));
        assert!(actual.contains("\x1b["));
        Ok(())
    }
}
