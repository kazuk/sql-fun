use std::{io::IsTerminal, path::Path, str::FromStr};

/// error related [`HighlighterTheme`]
#[derive(thiserror::Error, Debug)]
pub enum HighlighterThemeError {
    /// Color name invalid
    #[error("Can not to parse as color : {0}")]
    ParseColorError(String),
    /// file IO error
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    /// deserialize error
    #[error(transparent)]
    Serde(#[from] toml::de::Error),
}

/// terminal color settings
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, clap::ValueEnum)]
pub enum TerminalColor {
    /// enable ANSI Color if stderr is terminal
    Auto,
    /// disable ANSI Color (plain text)
    Never,
    /// ANSI Color terminal mode
    Ansi,
}

impl TerminalColor {
    /// get non auto value
    #[must_use]
    pub fn get_value(&self) -> Self {
        if matches!(self, Self::Auto) {
            if std::io::stderr().is_terminal() {
                Self::Ansi
            } else {
                Self::Never
            }
        } else {
            *self
        }
    }

    /// return true when self is never
    #[must_use]
    pub const fn is_never(&self) -> bool {
        matches!(self, Self::Never)
    }
}

/// item of highlighter element
#[expect(clippy::struct_excessive_bools)]
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct HighlighterStyle {
    foreground: Option<String>,
    background: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    dimmed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    strikethrough: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    hidden: bool,
}

#[expect(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !value
}

impl HighlighterStyle {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Highlighter theme
#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct HighlighterTheme {
    #[serde(default, skip_serializing_if = "HighlighterStyle::is_default")]
    identifier: HighlighterStyle,
    #[serde(default, skip_serializing_if = "HighlighterStyle::is_default")]
    constant: HighlighterStyle,
    #[serde(default, skip_serializing_if = "HighlighterStyle::is_default")]
    operator: HighlighterStyle,
    #[serde(default, skip_serializing_if = "HighlighterStyle::is_default")]
    keyword: HighlighterStyle,
    #[serde(default, skip_serializing_if = "HighlighterStyle::is_default")]
    delimiter: HighlighterStyle,
    #[serde(default, skip_serializing_if = "HighlighterStyle::is_default")]
    comment: HighlighterStyle,
}

impl HighlighterTheme {
    /// deserialize from toml text string
    ///
    /// # Errors
    ///
    /// [`HighlighterThemeError`] when deserialize error
    pub fn from_toml_str(toml: &str) -> Result<Self, HighlighterThemeError> {
        Ok(toml::from_str(toml)?)
    }

    /// get identifier style
    #[must_use]
    pub const fn identifier(&self) -> &HighlighterStyle {
        &self.identifier
    }

    /// get constant style
    #[must_use]
    pub const fn constant(&self) -> &HighlighterStyle {
        &self.constant
    }

    /// get operator style
    #[must_use]
    pub const fn operator(&self) -> &HighlighterStyle {
        &self.operator
    }

    /// get keyword style
    #[must_use]
    pub const fn keyword(&self) -> &HighlighterStyle {
        &self.keyword
    }

    /// get comment style
    #[must_use]
    pub const fn comment(&self) -> &HighlighterStyle {
        &self.comment
    }

    /// get delimiter style
    #[must_use]
    pub const fn delimiter(&self) -> &HighlighterStyle {
        &self.delimiter
    }
}

#[cfg(test)]
mod test_highlighter_theme_from_toml_str {
    use testresult::TestResult;

    use crate::HighlighterTheme;

    #[rstest::rstest]
    #[case(
        r#"
identifier = { foreground= "blue" }
constant = { foreground= "blue" }
comment = { foreground= "green", dimmed = true }
"#
    )]
    fn test_from_toml_str(#[case] toml: &str) -> TestResult {
        let _theme = HighlighterTheme::from_toml_str(toml)?;
        Ok(())
    }
}

impl HighlighterTheme {
    /// read toml formatted theme from path
    ///
    /// # Errors
    ///
    /// [`HighlighterThemeError`] when thema file IO / parse error.
    pub fn from_toml<P: AsRef<Path>>(path: P) -> Result<Self, HighlighterThemeError> {
        let toml = fs_err::read_to_string(path)?;
        Self::from_toml_str(&toml)
    }
}

impl HighlighterStyle {
    /// convert to [`owo_colors::Style`]
    ///
    /// # Errors
    ///
    /// [`HighlighterThemeError`] when syly item invalid
    pub fn get_style(&self) -> Result<owo_colors::Style, HighlighterThemeError> {
        let style = owo_colors::Style::default();
        let style = if let Some(foreground) = &self.foreground {
            let fore_color = owo_colors::DynColors::from_str(foreground)
                .map_err(|_| HighlighterThemeError::ParseColorError(foreground.clone()))?;
            style.color(fore_color)
        } else {
            style
        };
        let style = if let Some(background) = &self.background {
            let back_color = owo_colors::DynColors::from_str(background)
                .map_err(|_| HighlighterThemeError::ParseColorError(background.clone()))?;
            style.on_color(back_color)
        } else {
            style
        };

        if self.bold {
            style.bold()
        } else {
            style
        };
        let style = if self.dimmed { style.dimmed() } else { style };
        let style = if self.italic { style.italic() } else { style };
        let style = if self.strikethrough {
            style.strikethrough()
        } else {
            style
        };
        let style = if self.hidden { style.hidden() } else { style };

        Ok(style)
    }
}
