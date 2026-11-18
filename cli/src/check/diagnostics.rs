use std::io::Write;

use ::anyhow::Context;
use ::sql_fun_client::payloads::ReportLintExpectationsResponse;

use crate::{check::submission::QueryAnalysis, diagnostic::DiagnosticRenderer};

/// Present every SQL analysis diagnostic with its extracted Rust and SQL context.
pub(super) fn render_all(
    analysis: &QueryAnalysis,
    renderer: &DiagnosticRenderer,
    output: &mut impl Write,
) -> Result<(), ::anyhow::Error> {
    for item in analysis.response().queries().items() {
        for diagnostic in item.probrems().items() {
            let report = analysis.render(diagnostic, renderer)?;
            writeln!(
                output,
                "SQL query {} at {}:{}\n{}Original SQL:\n{}\n",
                analysis.function_name(),
                analysis.source_file(),
                analysis.source_line(),
                report,
                analysis.original_query(),
            )
            .with_context(|| format!("cannot write diagnostic for {}", analysis.function_name()))?;
        }
    }
    Ok(())
}

/// Render datasource-level warnings and indicate whether any expectation is unmet.
pub(super) fn render_lint_expectations(
    reports: &[(String, ReportLintExpectationsResponse)],
    renderer: &DiagnosticRenderer,
    output: &mut impl Write,
) -> Result<bool, ::anyhow::Error> {
    let mut unmet = false;
    for (datasource, report) in reports {
        for diagnostic in report.problems().items() {
            unmet |= diagnostic.code() == "unmet_lint_expectation";
            let problem = diagnostic.semantic_problem().as_ref().with_context(|| {
                format!("missing structured lint diagnostic for datasource {datasource}")
            })?;
            let message =
                renderer.render_problem(problem, (*diagnostic.severity()).into(), None, None)?;
            writeln!(output, "Datasource {datasource}\n{message}").with_context(|| {
                format!("cannot write lint diagnostic for datasource {datasource}")
            })?;
        }
    }
    Ok(unmet)
}
