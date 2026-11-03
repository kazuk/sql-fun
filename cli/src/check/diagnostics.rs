use std::io::Write;

use ::anyhow::Context;

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
