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
    /// Whether ANSI colors are explicitly disabled.
    #[must_use]
    pub const fn is_never(&self) -> bool {
        matches!(self, Self::Never)
    }
}
