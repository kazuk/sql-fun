mod expectation;
mod input;
mod report;
mod results;
mod test_input;

use crate::{DiagnosticRenderer, api::ApiArgs};

use self::{expectation::Expectation, input::SqlInputs, report::Case, test_input::TestInputs};
use crate::analysis::{initialization, prepare_diagnostics, prepare_tokens, request_typed};
use crate::diagnostic::terminal_color;
use std::{io::IsTerminal, path::PathBuf, process::ExitCode, time::Instant};

use ::anyhow::Context;
use ::sql_fun_client::{
    TcpJsonRpcClientAsync,
    payloads::{AnalyzeQueryArgs, AnalyzeQueryResponse, SchemaContext},
};
use ::sql_fun_core::{SchemaLevel, SqlFunArgs};

/// Analyze inline SQL or an explicit SQL file or structured selection.
#[derive(Debug, Clone, clap::Args)]
pub struct AnalyzeArgs {
    /// Literal SQL to analyze.
    #[arg(required_unless_present = "sql_file", conflicts_with = "sql_file")]
    sql: Option<String>,
    /// SQL filename, optionally followed by `#JSONPath`.
    #[arg(long, value_name = "FILE[#JSONPath]", required_unless_present = "sql")]
    sql_file: Option<String>,
    /// Cumulative schema prefix to load. Excluded SQL files are never accessed.
    #[arg(long, value_enum, default_value = "project")]
    schema_level: SchemaLevel,
    /// Disable relation/column access analysis and its output.
    #[arg(long)]
    no_column_access: bool,
    /// Additionally write SQL diagnostics to stderr.
    #[arg(long)]
    diagnostics_to_stderr: bool,
}
/// Check diagnostic sets and result-column contracts for explicit SQL inputs.
#[derive(Debug, Clone, clap::Args)]
pub struct TestArgs {
    #[command(flatten)]
    input: InputArgs,
    /// Legacy SQL expectations, optionally followed by `#JSONPath`.
    /// Without this option, INPUT is a JSON/TOML/YAML named case array,
    /// optionally followed by `#JSONPath`.
    #[arg(long)]
    expect: Option<String>,
    /// Write one `JUnit` testcase per selected SQL input to this file.
    #[arg(long)]
    junit_xml: Option<PathBuf>,
}
#[derive(Debug, Clone, clap::Args)]
struct InputArgs {
    /// Input filename or structured selection (`filename#JSONPath`).
    input: String,
    /// Cumulative schema prefix to load. Excluded SQL files are never accessed.
    #[arg(long, value_enum, default_value = "project")]
    schema_level: SchemaLevel,
}
impl AnalyzeArgs {
    pub(crate) async fn execute(&self, args: &SqlFunArgs) -> Result<ExitCode, ::anyhow::Error> {
        let inputs = if let Some(sql) = &self.sql {
            SqlInputs::inline(sql)
        } else if let Some(file) = &self.sql_file {
            SqlInputs::load(file)?
        } else {
            ::anyhow::bail!("SQL input is required")
        };
        let cases = execute(
            args,
            self.schema_level,
            &inputs,
            None,
            !self.no_column_access,
            self.diagnostics_to_stderr,
        )
        .await;
        finish(
            &cases,
            None,
            false,
            terminal_color(args.term_color(), std::io::stdout().is_terminal()),
        )
    }
}
impl TestArgs {
    pub(crate) async fn execute(&self, args: &SqlFunArgs) -> Result<ExitCode, ::anyhow::Error> {
        let inputs = TestInputs::load(&self.input.input, self.expect.as_deref())?;
        let cases = execute(
            args,
            self.input.schema_level,
            inputs.sql(),
            Some(inputs.expectations()),
            false,
            false,
        )
        .await;
        finish(&cases, self.junit_xml.as_deref(), true, false)
    }
}
fn finish(
    cases: &[Case],
    junit: Option<&std::path::Path>,
    testing: bool,
    color: bool,
) -> Result<ExitCode, ::anyhow::Error> {
    // Console errors must not prevent the requested report from being written.
    let console = (|| {
        for case in cases {
            if testing {
                case.print()?;
            } else {
                case.print_analysis(color)?;
            }
        }
        if testing {
            use std::io::Write;
            writeln!(
                std::io::stdout().lock(),
                "{} cases: {} passed, {} failed or errored",
                cases.len(),
                cases.iter().filter(|case| case.passed()).count(),
                cases.iter().filter(|case| !case.passed()).count()
            )?;
        }
        Ok::<(), std::io::Error>(())
    })();
    if let Some(path) = junit {
        report::write_report(path, cases)
            .with_context(|| format!("cannot write JUnit report {}", path.display()))?;
    }
    console?;
    Ok(if cases.iter().all(Case::passed) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

async fn execute(
    args: &SqlFunArgs,
    level: SchemaLevel,
    inputs: &SqlInputs,
    expectations: Option<&[Expectation]>,
    analyze_access: bool,
    diagnostics_to_stderr: bool,
) -> Vec<Case> {
    let mut cases: Vec<_> = inputs
        .queries()
        .iter()
        .enumerate()
        .map(|(index, _)| Case::new(&inputs.identity(index)))
        .collect();
    if expectations.is_none() {
        for case in &mut cases {
            case.enable_analysis();
        }
    }
    let setup = async {
        let datasource = args.datasource().context(
            "analyze/test require an explicitly selected --datasource (or SQL_FUN_DATASOURCE)",
        )?;
        let request = initialization(args, datasource, level)
            .with_context(|| format!("cannot set up datasource {datasource:?}"))?;
        let renderer = DiagnosticRenderer::from_args(args)?;
        let stdout_renderer = if expectations.is_none() {
            Some(DiagnosticRenderer::for_terminal(
                args,
                std::io::stdout().is_terminal(),
            )?)
        } else {
            None
        };
        let mut client = ApiArgs::from_environment()?.connect().await?;
        let initialized = initialize(&mut client, &request, &renderer).await;
        let initialized = match initialized {
            Ok(context) => context,
            Err(error) => {
                let _ = client.finish().await;
                return Err(error);
            }
        };
        Ok::<_, ::anyhow::Error>((client, initialized, renderer, stdout_renderer))
    }
    .await;
    let (client, context, renderer, stdout_renderer) = match setup {
        Ok(setup) => setup,
        Err(error) => {
            for case in &mut cases {
                case.set_error(&format!("schema setup: {error:#}"));
            }
            return cases;
        }
    };
    execute_session(
        client,
        context,
        SessionRenderers {
            stderr: renderer,
            stdout: stdout_renderer,
            duplicate: diagnostics_to_stderr,
        },
        inputs,
        expectations,
        analyze_access,
        cases,
    )
    .await
}

struct SessionRenderers {
    stderr: DiagnosticRenderer,
    stdout: Option<DiagnosticRenderer>,
    duplicate: bool,
}

async fn execute_session(
    mut client: TcpJsonRpcClientAsync,
    context: SchemaContext,
    renderers: SessionRenderers,
    inputs: &SqlInputs,
    expectations: Option<&[Expectation]>,
    analyze_access: bool,
    mut cases: Vec<Case>,
) -> Vec<Case> {
    for (index, query) in inputs.queries().iter().enumerate() {
        let start = Instant::now();
        let result = async {
            let handle = context
                .context_handle()
                .as_ref()
                .context("schema initialization returned no context handle")?;
            let request = AnalyzeQueryArgs::new(handle, query);
            let request = if analyze_access {
                request.with_access_analysis()
            } else {
                request
            };
            let response: AnalyzeQueryResponse =
                request_typed(&mut client, "analyze_query", &request).await?;
            let prepared = prepare_tokens(&mut client, &response).await;
            if let Some(expectations) = expectations {
                for (statement, item) in response.queries().items().iter().enumerate() {
                    for diagnostic in item.probrems().items() {
                        use std::io::Write;
                        writeln!(
                            std::io::stderr().lock(),
                            "{} (statement {statement})\n{}",
                            inputs.diagnostic_identity(index),
                            prepared.render(diagnostic, &renderers.stderr)?
                        )?;
                    }
                }
                if let Some(error) = prepared.error() {
                    ::anyhow::bail!("cannot tokenize SQL diagnostics: {error}");
                }
                Ok(expectations[index].differences(&response)?)
            } else {
                let report = cases[index]
                    .analysis_mut()
                    .context("missing analysis report")?;
                let output_renderer = renderers
                    .stdout
                    .as_ref()
                    .context("missing stdout renderer")?;
                let duplicate_renderer = if renderers.duplicate {
                    Some(&renderers.stderr)
                } else {
                    None
                };
                report.collect(
                    &response,
                    &prepared,
                    output_renderer,
                    duplicate_renderer,
                    analyze_access,
                )?;
                Ok(Vec::new())
            }
        }
        .await;
        match result {
            Ok(failures) => cases[index].set_failures(failures),
            Err(error) => cases[index].set_error(&format!("{error:#}")),
        }
        cases[index].set_duration(start.elapsed());
    }
    if let Err(error) = client.finish().await {
        let report_error = expectations.is_none() || !cases.iter().any(Case::has_error);
        if report_error && let Some(case) = cases.last_mut() {
            case.append_error(&format!("analysis session completion: {error}"));
        }
    }

    cases
}

#[cfg(test)]
mod test_execute_session {
    use crate::{
        direct_sql::{Case, Expectation, SessionRenderers, SqlInputs, execute_session},
        test_helpers::direct_sql::session as test_helpers,
    };
    #[tokio::test]
    async fn analyze_keeps_independent_outcomes_after_rpc_failure() -> ::testresult::TestResult {
        let fixture = test_helpers::SessionInputs::new(&["ok", "api_error", "ok"])?;
        let inputs = SqlInputs::load(&fixture.selection())?;
        let (client, peer) = test_helpers::peer().await;
        let cases = inputs
            .queries()
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let mut case = Case::new(&inputs.identity(index));
                case.enable_analysis();
                case
            })
            .collect();
        let cases = execute_session(
            client,
            test_helpers::context(),
            SessionRenderers {
                stderr: test_helpers::renderer()?,
                stdout: Some(test_helpers::renderer()?),
                duplicate: false,
            },
            &inputs,
            None,
            true,
            cases,
        )
        .await;
        peer.await?;
        assert_eq!(cases.iter().filter(|case| case.passed()).count(), 2);
        assert!(cases[1].has_error());
        Ok(())
    }

    #[rstest::rstest]
    #[case::rpc_continues(vec!["ok", "api_error", "ok"], 2, 1)]
    #[case::terminal_marks_remaining(vec!["ok", "transport_error", "ok"], 1, 2)]
    #[tokio::test]
    async fn independent_cases_keep_their_outcomes_after_execution_errors(
        #[case] queries: Vec<&str>,
        #[case] passes: usize,
        #[case] errors: usize,
    ) -> ::testresult::TestResult {
        let fixture = test_helpers::SessionInputs::new(&queries)?;
        let inputs = SqlInputs::load(&fixture.selection())?;
        let expected = Expectation::load(&fixture.expectations(), &inputs)?;
        let renderer = test_helpers::renderer()?;
        let (client, peer) = test_helpers::peer().await;
        let cases = inputs
            .queries()
            .iter()
            .enumerate()
            .map(|(index, _)| Case::new(&inputs.identity(index)))
            .collect();
        let cases = execute_session(
            client,
            test_helpers::context(),
            SessionRenderers {
                stderr: renderer,
                stdout: None,
                duplicate: false,
            },
            &inputs,
            Some(&expected),
            false,
            cases,
        )
        .await;
        peer.await?;
        assert_eq!(cases.iter().filter(|case| case.passed()).count(), passes);
        assert_eq!(cases.iter().filter(|case| case.has_error()).count(), errors);
        Ok(())
    }
}

async fn initialize(
    client: &mut TcpJsonRpcClientAsync,
    request: &::sql_fun_client::payloads::InitializeSchemaArgs,
    renderer: &DiagnosticRenderer,
) -> Result<SchemaContext, ::anyhow::Error> {
    let context: SchemaContext =
        request_typed(client, "initialize_schema_context", request).await?;
    let prepared = prepare_diagnostics(client, context.problems().items()).await;
    for diagnostic in context.problems().items() {
        use std::io::Write;
        writeln!(
            std::io::stderr().lock(),
            "schema initialization\n{}",
            prepared.render(diagnostic, renderer)?
        )?;
    }
    if let Some(error) = prepared.error() {
        ::anyhow::bail!("cannot tokenize schema diagnostics: {error}");
    }
    ::anyhow::ensure!(
        !context
            .problems()
            .items()
            .iter()
            .any(|diagnostic| *diagnostic.severity()
                == ::sql_fun_client::payloads::DiagnosticSeverity::error()),
        "schema initialization has error-level diagnostics"
    );
    ::anyhow::ensure!(
        context.context_handle().is_some(),
        "schema initialization returned no context handle"
    );
    Ok(context)
}
