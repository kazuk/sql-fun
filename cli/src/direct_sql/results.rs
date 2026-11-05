use crate::{analysis::PreparedDiagnostics, diagnostic::DiagnosticRenderer};
use std::io::Write;

use ::sql_fun_client::payloads::{
    AnalyzeQueryItem, AnalyzeQueryResponse, ColumnAccess, DiagnosticSeverity, ObjectKind,
    TypeReference,
};
use ::tabled::{builder::Builder, settings::Style};

/// Buffered statement output and severity for one independently selected input.
#[derive(Default)]
pub(super) struct AnalysisReport {
    body: Vec<u8>,
    diagnostics_stderr: Vec<u8>,
    severity: Status,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    #[default]
    Ok,
    Warn,
    Error,
}

impl AnalysisReport {
    pub(super) fn collect(
        &mut self,
        response: &AnalyzeQueryResponse,
        prepared: &PreparedDiagnostics,
        stdout_renderer: &DiagnosticRenderer,
        stderr_renderer: Option<&DiagnosticRenderer>,
        show_access: bool,
    ) -> Result<(), ::anyhow::Error> {
        let statements = response.queries().items();
        let mut errors = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            if statements.len() > 1 {
                writeln!(self.body, "\n  Statement {index}")?;
            }
            self.collect_statement(
                statement,
                prepared,
                stdout_renderer,
                stderr_renderer,
                show_access,
                &mut errors,
            )?;
        }
        if let Some(error) = prepared.error() {
            errors.push(format!("cannot tokenize SQL diagnostics: {error}"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            ::anyhow::bail!("{}", errors.join("\n"))
        }
    }
}

#[cfg(test)]
mod test_collect {
    use crate::{
        analysis::PreparedDiagnostics,
        direct_sql::results::AnalysisReport,
        test_helpers::direct_sql::{results::diagnostic_response, session},
    };

    #[rstest::rstest]
    #[case::single(1, false)]
    #[case::multiple(2, true)]
    fn labels_only_multiple_statements(
        #[case] count: usize,
        #[case] show_headings: bool,
    ) -> ::testresult::TestResult {
        let statements = vec![&[][..]; count];
        let response = diagnostic_response(&statements);
        let renderer = session::renderer()?;
        let mut report = AnalysisReport::default();
        report.collect(
            &response,
            &PreparedDiagnostics::default(),
            &renderer,
            None,
            true,
        )?;
        let body = String::from_utf8(report.body)?;
        assert_eq!(body.contains("Statement 0"), show_headings);
        assert_eq!(body.contains("Statement 1"), show_headings);
        Ok(())
    }
}

impl AnalysisReport {
    fn collect_statement(
        &mut self,
        statement: &AnalyzeQueryItem,
        prepared: &PreparedDiagnostics,
        stdout_renderer: &DiagnosticRenderer,
        stderr_renderer: Option<&DiagnosticRenderer>,
        show_access: bool,
        errors: &mut Vec<String>,
    ) -> std::io::Result<()> {
        writeln!(self.body, "\n  SQL")?;
        if let Some(sql) = statement.execute_sql() {
            for line in sql.lines() {
                writeln!(self.body, "    {line}")?;
            }
        } else {
            writeln!(self.body, "    unavailable")?;
        }
        self.collect_diagnostics(
            statement,
            prepared,
            stdout_renderer,
            stderr_renderer,
            errors,
        )?;
        write_parameters(&mut self.body, statement)?;
        write_columns(&mut self.body, statement)?;
        if show_access {
            write_accesses(&mut self.body, statement.column_access().items())?;
        }
        Ok(())
    }

    fn collect_diagnostics(
        &mut self,
        statement: &AnalyzeQueryItem,
        prepared: &PreparedDiagnostics,
        stdout_renderer: &DiagnosticRenderer,
        stderr_renderer: Option<&DiagnosticRenderer>,
        errors: &mut Vec<String>,
    ) -> std::io::Result<()> {
        if !statement.probrems().items().is_empty() {
            writeln!(self.body, "\n  Diagnostics")?;
        }
        for diagnostic in statement.probrems().items() {
            self.severity = self.severity.max(match diagnostic.severity() {
                DiagnosticSeverity::Error => Status::Error,
                DiagnosticSeverity::Warning => Status::Warn,
                DiagnosticSeverity::Advice => Status::Ok,
            });
            match prepared.render(diagnostic, stdout_renderer) {
                Ok(rendered) => {
                    for line in rendered.lines() {
                        writeln!(self.body, "    {line}")?;
                    }
                }
                Err(error) => errors.push(format!("cannot render SQL diagnostic: {error:#}")),
            }
            if let Some(renderer) = stderr_renderer {
                match prepared.render(diagnostic, renderer) {
                    Ok(rendered) => writeln!(self.diagnostics_stderr, "{rendered}")?,
                    Err(error) => errors.push(format!(
                        "cannot render SQL diagnostic for stderr: {error:#}"
                    )),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_collect_diagnostics {
    use crate::{
        analysis::PreparedDiagnostics,
        direct_sql::results::{AnalysisReport, Status},
        test_helpers::direct_sql::{results::diagnostic_response, session},
    };
    use ::sql_fun_client::payloads::DiagnosticSeverity as Severity;

    #[rstest::rstest]
    #[case::none(vec![vec![]], Status::Ok)]
    #[case::advice(vec![vec![Severity::advice()]], Status::Ok)]
    #[case::warning(vec![vec![Severity::warning()]], Status::Warn)]
    #[case::error_after_warning(vec![vec![Severity::warning()], vec![Severity::error()]], Status::Error)]
    fn aggregates_and_duplicates_diagnostics(
        #[case] severities: Vec<Vec<Severity>>,
        #[case] expected: Status,
    ) -> ::testresult::TestResult {
        let severities = severities.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let response = diagnostic_response(&severities);
        let diagnostic_count: usize = severities.iter().map(|items| items.len()).sum();
        let renderer = session::renderer()?;
        let mut report = AnalysisReport::default();
        let mut errors = Vec::new();
        for statement in response.queries().items() {
            report.collect_diagnostics(
                statement,
                &PreparedDiagnostics::default(),
                &renderer,
                Some(&renderer),
                &mut errors,
            )?;
        }
        assert!(report.severity == expected);
        assert_eq!(errors, Vec::<String>::new());
        let stdout = String::from_utf8(report.body)?;
        let stderr = String::from_utf8(report.diagnostics_stderr)?;
        assert_eq!(
            stdout.matches("parameter_constraint").count(),
            diagnostic_count
        );
        assert_eq!(
            stderr.matches("parameter_constraint").count(),
            diagnostic_count
        );
        assert!(!stderr.contains("Statement"));
        Ok(())
    }
}

impl AnalysisReport {
    pub(super) fn passed(&self) -> bool {
        self.severity != Status::Error
    }
}

#[cfg(test)]
mod test_passed {
    use crate::direct_sql::results::{AnalysisReport, Status};

    #[rstest::rstest]
    #[case::ok(Status::Ok, true)]
    #[case::warning(Status::Warn, true)]
    #[case::error(Status::Error, false)]
    fn errors_fail_the_report(#[case] severity: Status, #[case] expected: bool) {
        let report = AnalysisReport {
            severity,
            ..AnalysisReport::default()
        };
        assert_eq!(report.passed(), expected);
    }
}

impl AnalysisReport {
    pub(super) fn write(
        &self,
        output: &mut impl Write,
        stderr: &mut impl Write,
        name: &str,
        error: Option<&str>,
        color: bool,
    ) -> std::io::Result<()> {
        let status = if error.is_some() {
            Status::Error
        } else {
            self.severity
        };
        let (label, ansi) = match status {
            Status::Ok => ("OK", 32),
            Status::Warn => ("WARN", 33),
            Status::Error => ("ERROR", 31),
        };
        if color {
            writeln!(output, "\x1b[{ansi}m{label}\x1b[0m {name}")?;
        } else {
            writeln!(output, "{label} {name}")?;
        }
        output.write_all(&self.body)?;
        if let Some(error) = error {
            writeln!(output, "\n  Operational error:")?;
            for line in error.lines() {
                writeln!(output, "    {line}")?;
            }
        }
        writeln!(output)?;
        stderr.write_all(&self.diagnostics_stderr)?;
        Ok(())
    }
}

#[cfg(test)]
mod test_write {
    use crate::direct_sql::results::{AnalysisReport, Status};

    #[rstest::rstest]
    #[case::ok(Status::Ok, "\x1b[32mOK\x1b[0m")]
    #[case::warn(Status::Warn, "\x1b[33mWARN\x1b[0m")]
    #[case::error(Status::Error, "\x1b[31mERROR\x1b[0m")]
    fn applies_status_colors(
        #[case] severity: Status,
        #[case] expected: &str,
    ) -> ::testresult::TestResult {
        let report = AnalysisReport {
            severity,
            ..AnalysisReport::default()
        };
        let mut stdout = Vec::new();
        report.write(&mut stdout, &mut Vec::new(), "input", None, true)?;
        assert!(String::from_utf8(stdout)?.starts_with(expected));
        Ok(())
    }

    #[test]
    fn reports_operational_error_without_losing_results() -> ::testresult::TestResult {
        let report = AnalysisReport {
            body: b"\n  Result columns\n    id  unresolved\n".to_vec(),
            ..AnalysisReport::default()
        };
        let mut stdout = Vec::new();
        report.write(
            &mut stdout,
            &mut Vec::new(),
            "input",
            Some("transport failure"),
            false,
        )?;
        let stdout = String::from_utf8(stdout)?;
        assert!(stdout.starts_with("ERROR input"));
        assert!(stdout.contains("Operational error:\n    transport failure"));
        assert!(stdout.contains("id  unresolved"));
        Ok(())
    }
}

fn write_table(
    output: &mut impl Write,
    title: &str,
    headings: &[&str],
    rows: Vec<Vec<String>>,
) -> std::io::Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = Builder::new();
    builder.push_record(headings.iter().copied());
    for row in rows {
        builder.push_record(row);
    }
    let mut table = builder.build();
    table.with(Style::blank());
    writeln!(output, "\n  {title}")?;
    for line in table.to_string().lines() {
        writeln!(output, "   {line}")?;
    }
    Ok(())
}

#[cfg(test)]
mod test_write_table {
    use crate::direct_sql::results::write_table;

    #[test]
    fn keeps_long_unicode_and_multiline_values_without_width_limits() -> ::testresult::TestResult {
        let long = "x".repeat(160);
        let mut output = Vec::new();
        write_table(
            &mut output,
            "Values",
            &["Name", "Type"],
            vec![vec!["名前\nsecond".to_owned(), long.clone()]],
        )?;
        let output = String::from_utf8(output)?;
        assert!(output.contains("名前"));
        assert!(output.contains("second"));
        assert!(output.contains(&long));
        Ok(())
    }
}

fn write_parameters(output: &mut impl Write, statement: &AnalyzeQueryItem) -> std::io::Result<()> {
    let mut parameters: Vec<_> = statement.parameters().items().iter().collect();
    parameters.sort_by_key(|parameter| *parameter.parameter_index());
    let rows = parameters
        .into_iter()
        .map(|parameter| {
            vec![
                format!("${}", parameter.parameter_index()),
                parameter.name().to_owned(),
                type_name(parameter.parameter_type().as_ref()),
                parameter.nullability().contract_name().to_owned(),
            ]
        })
        .collect();
    write_table(
        output,
        "Parameters",
        &["#", "Name", "Type", "Nullability"],
        rows,
    )
}

#[cfg(test)]
mod test_write_parameters {
    use crate::{
        direct_sql::results::write_parameters,
        test_helpers::direct_sql::results::column_access_response,
    };
    #[test]
    fn sorts_parameters_by_placeholder_index() -> ::testresult::TestResult {
        let response = column_access_response();
        let mut output = Vec::new();
        write_parameters(&mut output, &response.queries().items()[0])?;
        let output = String::from_utf8(output)?;
        assert!(output.find("first").unwrap() < output.find("third").unwrap());
        assert!(output.contains("$1") && output.contains("$3"));
        Ok(())
    }
}

fn write_columns(output: &mut impl Write, statement: &AnalyzeQueryItem) -> std::io::Result<()> {
    let mut columns: Vec<_> = statement.result_columns().items().iter().collect();
    columns.sort_by_key(|column| *column.ordinal());
    let rows = columns
        .into_iter()
        .map(|column| {
            vec![
                column.ordinal().to_string(),
                column.name().to_owned(),
                type_name(column.column_type().as_ref()),
                column.nullability().contract_name().to_owned(),
            ]
        })
        .collect();
    write_table(
        output,
        "Result columns",
        &["#", "Name", "Type", "Nullability"],
        rows,
    )
}

#[cfg(test)]
mod test_write_columns {
    use crate::{
        direct_sql::results::write_columns,
        test_helpers::direct_sql::results::column_access_response,
    };
    #[test]
    fn sorts_columns_by_ordinal_and_displays_types() -> ::testresult::TestResult {
        let response = column_access_response();
        let mut output = Vec::new();
        write_columns(&mut output, &response.queries().items()[0])?;
        let output = String::from_utf8(output)?;
        assert!(output.find(" id ").unwrap() < output.find(" ids ").unwrap());
        assert!(output.contains("unresolved") && output.contains("pg_catalog.int4[]"));
        Ok(())
    }
}

fn write_accesses(output: &mut impl Write, records: &[ColumnAccess]) -> std::io::Result<()> {
    let mut accesses: Vec<_> = records.iter().collect();
    accesses.sort_by_key(|access| {
        let relation = access.relation();
        (
            relation.catalog(),
            relation.schema(),
            relation.name(),
            access.column_name(),
            access.relation_kind().label(),
            access.access_kind().name(),
        )
    });
    let rows = accesses
        .into_iter()
        .map(|access| {
            let relation = access.relation();
            let mut name = relation
                .catalog()
                .iter()
                .chain(relation.schema())
                .map(String::as_str)
                .chain(std::iter::once(relation.name().as_str()))
                .collect::<Vec<_>>()
                .join(".");
            if let Some(column) = access.column_name() {
                name.push('.');
                name.push_str(column);
            }
            vec![
                name,
                access.relation_kind().label().to_owned(),
                access.access_kind().name().to_owned(),
            ]
        })
        .collect();
    write_table(
        output,
        "Column access",
        &["Relation/column", "Kind", "Role"],
        rows,
    )
}

#[cfg(test)]
mod test_write_accesses {
    use crate::{
        direct_sql::results::write_accesses,
        test_helpers::direct_sql::results::column_access_response,
    };
    #[test]
    fn sorts_accesses_and_preserves_repeated_roles() -> ::testresult::TestResult {
        let response = column_access_response();
        let mut output = Vec::new();
        write_accesses(
            &mut output,
            response.queries().items()[0].column_access().items(),
        )?;
        let output = String::from_utf8(output)?;
        assert!(output.find("ReadRows").unwrap() < output.find("SelectFilter").unwrap());
        assert!(output.find("SelectFilter").unwrap() < output.find("SelectOutput").unwrap());
        assert_eq!(output.matches("SelectOutput").count(), 2);
        assert!(output.contains("app.public.events.id"));
        Ok(())
    }
}

fn type_name(reference: Option<&TypeReference>) -> String {
    match reference {
        Some(reference) => format!(
            "{}.{}{}",
            reference.schema_name(),
            reference.local_name(),
            if *reference.is_array() { "[]" } else { "" }
        ),
        None => "unresolved".to_owned(),
    }
}

#[cfg(test)]
mod test_type_display {
    use crate::direct_sql::results::{TypeReference, type_name};
    #[rstest::rstest]
    #[case::scalar(Some(false), "pg_catalog.int4")]
    #[case::array(Some(true), "pg_catalog.int4[]")]
    #[case::unresolved(None, "unresolved")]
    fn distinguishes_unresolved_scalar_and_array_types(
        #[case] array: Option<bool>,
        #[case] expected: &str,
    ) {
        let reference = array.map(|array| TypeReference::new("pg_catalog", "int4", array));
        assert_eq!(type_name(reference.as_ref()), expected);
    }
}

trait AnalysisLabel {
    fn label(&self) -> &'static str;
}
impl AnalysisLabel for ObjectKind {
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Table => "Table",
            Self::View => "View",
            Self::Type => "Type",
            Self::Function => "Function",
            Self::MaterializedView => "MaterializedView",
            Self::Sequence => "Sequence",
            Self::Trigger => "Trigger",
            Self::Index => "Index",
        }
    }
}
