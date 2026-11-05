use crate::direct_sql::{expectation::Difference, results::AnalysisReport};
use std::{io::Write, path::Path, time::Duration};

use ::quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, BytesText, Event},
};

pub(super) struct Case {
    name: String,
    duration: Duration,
    failures: Vec<Difference>,
    diagnostic_context: Vec<String>,
    error: Option<String>,
    analysis: Option<AnalysisReport>,
}
impl Case {
    pub(super) fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            duration: Duration::ZERO,
            failures: Vec::new(),
            diagnostic_context: Vec::new(),
            error: None,
            analysis: None,
        }
    }
    pub(super) fn enable_analysis(&mut self) {
        self.analysis = Some(AnalysisReport::default());
    }
    pub(super) const fn analysis_mut(&mut self) -> Option<&mut AnalysisReport> {
        self.analysis.as_mut()
    }
    pub(super) fn print_analysis(&self, color: bool) -> std::io::Result<()> {
        if let Some(report) = &self.analysis {
            report.write(
                &mut std::io::stdout().lock(),
                &mut std::io::stderr().lock(),
                &self.name,
                self.error.as_deref(),
                color,
            )?;
        }
        Ok(())
    }
    pub(super) const fn set_duration(&mut self, duration: Duration) {
        self.duration = duration;
    }
    pub(super) fn set_failures(&mut self, failures: Vec<Difference>) {
        self.failures = failures;
    }
    pub(super) fn set_diagnostic_context(&mut self, context: Vec<String>) {
        self.diagnostic_context = context;
    }
    pub(super) fn set_error(&mut self, error: &str) {
        self.error = Some(error.to_owned());
    }
    pub(super) fn append_error(&mut self, error: &str) {
        if let Some(existing) = &mut self.error {
            existing.push('\n');
            existing.push_str(error);
        } else {
            self.set_error(error);
        }
    }
    pub(super) const fn has_error(&self) -> bool {
        self.error.is_some()
    }
    pub(super) fn passed(&self) -> bool {
        self.error.is_none()
            && self.failures.is_empty()
            && self.analysis.as_ref().is_none_or(AnalysisReport::passed)
    }
    pub(super) fn print(&self, color: bool) -> std::io::Result<()> {
        self.write_console(&mut std::io::stdout().lock(), color)
    }
    fn write_console(&self, output: &mut impl Write, color: bool) -> std::io::Result<()> {
        let status = if self.error.is_some() {
            "ERROR"
        } else if self.failures.is_empty() {
            "PASS"
        } else {
            "FAIL"
        };
        writeln!(output, "{status} {}", self.name)?;
        for message in self
            .error
            .iter()
            .cloned()
            .chain(self.failures.iter().map(|failure| failure.format(color)))
            .chain(self.diagnostic_context.iter().cloned())
        {
            for line in message.lines() {
                writeln!(output, "  {line}")?;
            }
        }
        if !self.passed() {
            writeln!(output)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_console {
    use crate::direct_sql::{expectation::Difference, report::Case};

    #[test]
    fn indents_each_detail_line_and_separates_failure_cases() -> ::testresult::TestResult {
        let mut failed = Case::new("query[0]");
        failed.set_failures(vec![Difference::ColumnField {
            index: 0,
            field: "name",
            expected: "id".into(),
            actual: "other".into(),
        }]);
        let mut error = Case::new("query[1]");
        error.set_error("transport failure\nconnection closed");
        let mut output = Vec::new();
        failed.write_console(&mut output, false)?;
        error.write_console(&mut output, false)?;
        Case::new("query[2]").write_console(&mut output, false)?;
        assert_eq!(
            String::from_utf8(output)?,
            concat!(
                "FAIL query[0]\n  Column 0 · name\n    Expected: id\n    Actual:   other\n\n",
                "ERROR query[1]\n  transport failure\n  connection closed\n\n",
                "PASS query[2]\n",
            )
        );
        Ok(())
    }
}

/// Replace XML 1.0-disallowed characters in report text without dropping the case.
fn xml_text(value: &str) -> String {
    value.chars().map(|character| if matches!(character, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}') { character } else { '\u{fffd}' }).collect()
}

pub(super) fn write_report(path: &Path, cases: &[Case]) -> Result<(), ::anyhow::Error> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    writer.write_event(Event::Start(BytesStart::new("testsuites")))?;
    let tests = cases.len().to_string();
    let failures = cases
        .iter()
        .filter(|case| case.error.is_none() && !case.failures.is_empty())
        .count()
        .to_string();
    let errors = cases
        .iter()
        .filter(|case| case.error.is_some())
        .count()
        .to_string();
    let duration = cases
        .iter()
        .map(|case| case.duration.as_secs_f64())
        .sum::<f64>()
        .to_string();
    let mut suite = BytesStart::new("testsuite");
    suite.push_attribute(("name", "sql-fun test"));
    suite.push_attribute(("tests", tests.as_str()));
    suite.push_attribute(("failures", failures.as_str()));
    suite.push_attribute(("errors", errors.as_str()));
    suite.push_attribute(("time", duration.as_str()));
    writer.write_event(Event::Start(suite))?;
    for case in cases {
        let name = xml_text(&case.name);
        let time = case.duration.as_secs_f64().to_string();
        let mut element = BytesStart::new("testcase");
        element.push_attribute(("classname", "sql-fun"));
        // Attribute tuples are escaped by quick-xml, as is text via BytesText::new.
        element.push_attribute(("name", name.as_str()));
        element.push_attribute(("time", time.as_str()));
        writer.write_event(Event::Start(element))?;
        let problem = if let Some(error) = &case.error {
            let details = if case.failures.is_empty() {
                error.clone()
            } else {
                format!(
                    "{error}\n{}",
                    case.failures
                        .iter()
                        .map(|failure| failure.format(false))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };
            Some(("error", details))
        } else if case.failures.is_empty() {
            None
        } else {
            Some((
                "failure",
                case.failures
                    .iter()
                    .map(|failure| failure.format(false))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ))
        };
        if let Some((kind, message)) = problem {
            writer.write_event(Event::Start(BytesStart::new(kind)))?;
            writer.write_event(Event::Text(BytesText::new(&xml_text(&message))))?;
            writer.write_event(Event::End(BytesEnd::new(kind)))?;
        }
        writer.write_event(Event::End(BytesEnd::new("testcase")))?;
    }
    writer.write_event(Event::End(BytesEnd::new("testsuite")))?;
    writer.write_event(Event::End(BytesEnd::new("testsuites")))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, writer.into_inner())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::direct_sql::{
        expectation::Difference,
        report::{Case, write_report},
    };
    #[test]
    fn report_is_parseable_with_pass_failure_error_and_special_text() -> ::testresult::TestResult {
        let directory = ::tempfile::tempdir()?;
        let path = directory.path().join("nested/junit.xml");
        let mut failed = Case::new("source<&\"[1]");
        failed.set_failures(vec![Difference::ColumnField {
            index: 0,
            field: "name",
            expected: "<id> &".into(),
            actual: "名前\0".into(),
        }]);
        let mut error = Case::new("source[2]");
        error.set_error("transport failure");
        write_report(&path, &[Case::new("source[0]"), failed, error])?;
        let text = std::fs::read_to_string(path)?;
        assert!(!text.contains('\0'));
        assert!(text.contains('\u{fffd}'));
        let mut reader = ::quick_xml::Reader::from_str(&text);
        let mut cases = 0;
        let mut failures = 0;
        let mut errors = 0;
        loop {
            match reader.read_event()? {
                ::quick_xml::events::Event::Start(element) => match element.name().as_ref() {
                    "testcase" => {
                        cases += 1;
                        for attribute in element.attributes() {
                            let attribute = attribute?;
                            attribute.normalized_value(::quick_xml::XmlVersion::Implicit1_0)?;
                        }
                    }
                    "failure" => failures += 1,
                    "error" => errors += 1,
                    _ => {}
                },
                ::quick_xml::events::Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!((cases, failures, errors), (3, 1, 1));
        Ok(())
    }
}
