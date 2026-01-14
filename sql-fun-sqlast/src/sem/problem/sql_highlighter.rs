use std::ops::Range;

use miette::highlighters::{Highlighter, HighlighterState};
use owo_colors::{Style, Styled};
use pg_query::protobuf::{ScanToken, Token};
use sql_fun_core::HighlighterTheme;

use crate::sem::AnalysisError;

pub(crate) struct SqlHighlighter {
    style_ident: Style,
    style_const: Style,
    style_operator: Style,
    style_keyword: Style,
    style_delimiter: Style,
    style_comment: Style,
}

impl SqlHighlighter {
    pub(crate) fn from_theme(theme: &HighlighterTheme) -> Result<Self, AnalysisError> {
        Ok(Self {
            style_ident: theme.identifier().get_style()?,
            style_const: theme.constant().get_style()?,
            style_operator: theme.operator().get_style()?,
            style_keyword: theme.keyword().get_style()?,
            style_comment: theme.comment().get_style()?,
            style_delimiter: theme.delimiter().get_style()?,
        })
    }
}

impl Highlighter for SqlHighlighter {
    fn start_highlighter_state<'h>(
        &'h self,
        source: &dyn miette::SpanContents<'_>,
    ) -> Box<dyn miette::highlighters::HighlighterState + 'h> {
        let source_text = String::from(str::from_utf8(source.data()).expect("expect utf8"));
        let scan_result = pg_query::scan(&source_text).expect("tokens");

        Box::new(SqlHighlighterState {
            source_text,
            consumed: 0,
            tokens: scan_result.tokens,
            style_ident: self.style_ident,
            style_const: self.style_const,
            style_operator: self.style_operator,
            style_keyword: self.style_keyword,
            style_delimiter: self.style_delimiter,
            style_comment: self.style_comment,
        })
    }
}

pub struct SqlHighlighterState {
    source_text: String,
    consumed: usize,
    tokens: Vec<ScanToken>,
    style_ident: Style,
    style_const: Style,
    style_operator: Style,
    style_keyword: Style,
    style_delimiter: Style,
    style_comment: Style,
}

impl SqlHighlighterState {
    fn get_style_for_token(&self, token: &ScanToken) -> &Style {
        let token_kind = token.token();

        match token_kind {
            Token::Ident | Token::Uident => &self.style_ident,
            Token::CComment | Token::SqlComment => &self.style_comment,
            Token::Fconst
            | Token::Sconst
            | Token::Usconst
            | Token::Bconst
            | Token::Xconst
            | Token::Iconst => &self.style_const,
            Token::Op => &self.style_operator,
            _ => &self.style_keyword,
        }
    }

    fn to_slice_range(token_start: i32, token_end: i32) -> Range<usize> {
        (token_start as usize)..(token_end as usize)
    }

    fn to_styled_token<'s>(
        &self,
        tok: &ScanToken,
        line: &'s str,
        start_index: i32,
        line_len: i32,
    ) -> Styled<&'s str> {
        let token_start = tok.start - start_index;
        let token_start = if token_start < 0 { 0 } else { token_start };
        let token_end = tok.end - start_index;
        let token_end = if token_end > line_len {
            line_len
        } else {
            token_end
        };
        let style = self.get_style_for_token(tok);
        style.style(&line[Self::to_slice_range(token_start, token_end)])
    }

    fn to_delimiter_token<'s>(
        &self,
        line: &'s str,
        start_index: i32,
        end_index: i32,
    ) -> Styled<&'s str> {
        let style = self.style_delimiter;
        style.style(&line[Self::to_slice_range(start_index, end_index)])
    }
}

impl HighlighterState for SqlHighlighterState {
    fn highlight_line<'s>(&mut self, line: &'s str) -> Vec<Styled<&'s str>> {
        let line_len = line.len() as i32;
        let start_index = self.source_text[self.consumed..]
            .find(line)
            .expect("line in source_text");
        let start_index = (self.consumed + start_index) as i32;
        let end_index = start_index + line_len;
        let mut last_index = self.consumed as i32;

        self.consumed = end_index as usize;

        let mut styled = Vec::new();

        for tok in &self.tokens {
            if start_index < tok.end && tok.start < end_index {
                if last_index < start_index {
                    styled.push(self.to_delimiter_token(line, last_index, start_index))
                }
                styled.push(self.to_styled_token(tok, line, start_index, line_len));
                last_index = tok.end;
            }
        }

        styled
    }
}
