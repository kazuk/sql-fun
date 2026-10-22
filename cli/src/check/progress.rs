use std::io::Write;

/// Query-function progress granularity, independent of diagnostic rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(super) enum ProgressMode {
    Silent,
    Dots,
    Names,
}

impl ProgressMode {
    #[cfg(test)]
    pub(super) const fn silent() -> Self {
        Self::Silent
    }
    #[cfg(test)]
    pub(super) const fn dots() -> Self {
        Self::Dots
    }
    #[cfg(test)]
    pub(super) const fn names() -> Self {
        Self::Names
    }

    const fn visible(self) -> bool {
        !matches!(self, Self::Silent)
    }

    fn report_function(
        self,
        output: &mut impl Write,
        name: &str,
        completed: usize,
        total: usize,
    ) -> std::io::Result<bool> {
        match self {
            Self::Silent => Ok(false),
            Self::Dots => {
                write!(output, ".")?;
                Ok(true)
            }
            Self::Names => {
                writeln!(output, "[{completed}/{total}] {name}")?;
                Ok(false)
            }
        }
    }
}

/// Owns progress counts and line framing. All output is written to stdout.
pub(super) struct Progress<W: Write> {
    output: W,
    mode: ProgressMode,
    total: usize,
    completed: usize,
    dots_open: bool,
}

impl<W: Write> Progress<W> {
    pub(super) const fn new(output: W, mode: ProgressMode) -> Self {
        Self {
            output,
            mode,
            total: 0,
            completed: 0,
            dots_open: false,
        }
    }

    pub(super) fn phase(&mut self, phase: &str) -> std::io::Result<()> {
        self.end_line()?;
        if self.mode.visible() {
            writeln!(self.output, "{phase}")?;
            self.output.flush()?;
        }
        Ok(())
    }

    pub(super) const fn set_total(&mut self, total: usize) {
        self.total = total;
    }

    pub(super) fn begin(&mut self, total: usize) -> std::io::Result<()> {
        self.total = total;
        self.phase(&format!("Analyzing {total} query functions"))
    }

    pub(super) fn received(&mut self, name: &str) -> std::io::Result<()> {
        self.completed += 1;
        self.dots_open |=
            self.mode
                .report_function(&mut self.output, name, self.completed, self.total)?;
        self.output.flush()
    }

    fn end_line(&mut self) -> std::io::Result<()> {
        if self.dots_open {
            writeln!(self.output)?;
            self.dots_open = false;
        }
        Ok(())
    }

    pub(super) fn finish(&mut self, success: bool) -> std::io::Result<()> {
        let status = if success { "Completed" } else { "Stopped" };
        if self.total == 0 {
            return self.phase(status);
        }
        self.phase(&format!(
            "{status}: {}/{} query functions analyzed",
            self.completed, self.total
        ))
    }
}

#[cfg(test)]
mod test_progress {
    use super::{Progress, ProgressMode};

    #[rstest::rstest]
    #[case::silent(ProgressMode::silent(), "")]
    #[case::dots(
        ProgressMode::dots(),
        "Analyzing 2 query functions\n..\nCompleted: 2/2 query functions analyzed\n"
    )]
    #[case::names(
        ProgressMode::names(),
        "Analyzing 2 query functions\n[1/2] first\n[2/2] second\nCompleted: 2/2 query functions analyzed\n"
    )]
    fn completes_per_function(#[case] mode: ProgressMode, #[case] expected: &str) {
        let mut output = Vec::new();
        let mut reporter = Progress::new(&mut output, mode);
        reporter.begin(2).unwrap();
        reporter.received("first").unwrap();
        reporter.received("second").unwrap();
        reporter.finish(true).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }

    #[test]
    fn terminates_dots_on_failure_with_actual_count() {
        let mut output = Vec::new();
        let mut reporter = Progress::new(&mut output, ProgressMode::dots());
        reporter.begin(3).unwrap();
        reporter.received("first").unwrap();
        reporter.finish(false).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Analyzing 3 query functions\n.\nStopped: 1/3 query functions analyzed\n"
        );
    }
}
