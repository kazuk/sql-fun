use std::io::Write;

use ::anyhow::Context;

use super::submission::QueryAnalysis;
use crate::diagnostic::DiagnosticRenderer;

/// Present every SQL analysis diagnostic with its extracted Rust and SQL context.
pub(super) fn render_all(
    analysis: &QueryAnalysis,
    renderer: &DiagnosticRenderer,
    output: &mut impl Write,
) -> Result<(), ::anyhow::Error> {
    for item in analysis.response().queries().items() {
        for diagnostic in item.probrems().items() {
            let report = if let Some(presentation) = diagnostic.presentation() {
                renderer.render_problem(
                    presentation.problem(),
                    (*diagnostic.severity()).into(),
                    Some(presentation.source()),
                    analysis.tokens(presentation.source(), presentation.dialect()),
                )?
            } else if let Some(problem) = diagnostic.semantic_problem() {
                renderer.render_problem(problem, (*diagnostic.severity()).into(), None, None)?
            } else {
                ::anyhow::bail!(
                    "analysis diagnostic {} for {} lacks a structured problem",
                    diagnostic.code(),
                    analysis.function_name()
                );
            };
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
