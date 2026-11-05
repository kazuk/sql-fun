mod sql_highlighter;

use self::sql_highlighter::SqlHighlighter;
use crate::highlighter_theme::HighlighterTheme;
use std::io::IsTerminal;

use ::owo_colors::OwoColorize;
use ::sql_fun_core::{
    SqlFunArgs, TerminalColor, TokenStream,
    analysis_problem::{
        AnalysisProblem, DiagnosticMessage, DiagnosticMessageFragment, DiagnosticSeverity,
    },
};

/// CLI-owned diagnostic rendering, including terminal and theme configuration.
pub struct DiagnosticRenderer {
    theme: HighlighterTheme,
    color: bool,
}

impl DiagnosticRenderer {
    /// Resolve the requested presentation settings in the CLI process.
    ///
    /// # Errors
    /// Returns an error when the selected theme cannot be loaded.
    pub fn from_args(args: &SqlFunArgs) -> Result<Self, anyhow::Error> {
        Self::for_terminal(args, std::io::stderr().is_terminal())
    }

    /// Resolve diagnostics for an output stream independently of stderr.
    pub(crate) fn for_terminal(args: &SqlFunArgs, terminal: bool) -> Result<Self, anyhow::Error> {
        let color = terminal_color(args.term_color(), terminal);
        let theme = if color {
            let path = match args.highlighter_theme_path() {
                Some(path) if path.is_absolute() => path.to_path_buf(),
                Some(path) => args.sql_fun_home()?.join("highlighter").join(path),
                None => args.sql_fun_home()?.join("highlighter/default.toml"),
            };
            HighlighterTheme::from_toml(path)?
        } else {
            HighlighterTheme::default()
        };
        Ok(Self { theme, color })
    }

    /// Render a diagnostic against its complete statement token stream.
    ///
    /// # Errors
    /// Returns an error for invalid theme colors or report formatting failures.
    pub fn render(
        &self,
        problem: &AnalysisProblem,
        tokens: &TokenStream,
    ) -> Result<String, anyhow::Error> {
        self.render_problem(
            problem,
            problem.severity(),
            Some(tokens.source()),
            Some(tokens),
        )
    }

    fn message(&self, problem: &AnalysisProblem) -> Result<(String, bool), anyhow::Error> {
        if !self.color {
            return Ok((problem.diagnostic_message(), false));
        }
        let style = self.theme.identifier().style()?;
        let mut message = String::new();
        let mut has_names = false;
        for fragment in problem.diagnostic_fragments() {
            use std::fmt::Write;
            match fragment {
                DiagnosticMessageFragment::Text(text) => message.push_str(&text),
                name => {
                    has_names = true;
                    for part in name.as_str().split_inclusive(char::is_whitespace) {
                        write!(message, "{}", part.style(style))?;
                    }
                }
            }
        }
        Ok((message, has_names))
    }

    /// Render a structured problem even when source positions or tokens are absent.
    /// Labels are attached only to the exact retained statement source.
    pub(crate) fn render_problem(
        &self,
        problem: &AnalysisProblem,
        diagnostic_severity: DiagnosticSeverity,
        source: Option<&str>,
        tokens: Option<&TokenStream>,
    ) -> Result<String, anyhow::Error> {
        let metadata = problem.render_metadata();
        let severity = match diagnostic_severity {
            DiagnosticSeverity::Error => ::miette::Severity::Error,
            DiagnosticSeverity::Warning => ::miette::Severity::Warning,
            DiagnosticSeverity::Advice => ::miette::Severity::Advice,
        };
        let (message, has_names) = self.message(problem)?;
        let mut diagnostic = ::miette::MietteDiagnostic::new(message)
            .with_code(problem.code())
            .with_severity(severity);
        if source.is_some()
            && let Some(metadata) = &metadata
        {
            for label in metadata.labels() {
                let span = label.span();
                diagnostic = diagnostic.with_label(::miette::LabeledSpan::new(
                    Some(label.message().to_owned()),
                    span.offset(),
                    span.len(),
                ));
            }
        }
        let mut report = ::miette::Report::new(diagnostic);
        if let Some(source) = source {
            report = report.with_source_code(::miette::NamedSource::new("SQL", source.to_owned()));
        }
        let theme = if self.color {
            ::miette::GraphicalTheme::unicode()
        } else {
            ::miette::GraphicalTheme::unicode_nocolor()
        };
        let mut handler = ::miette::GraphicalReportHandler::new().with_theme(theme);
        if has_names {
            // Miette's continuation indent resets ANSI styles. Keep each styled
            // word intact so the following word can restore its identifier style.
            handler = handler.with_break_words(false);
        }
        if let Some(tokens) = tokens {
            handler =
                handler.with_syntax_highlighting(SqlHighlighter::from_theme(&self.theme, tokens)?);
        }
        let mut output = String::new();
        handler.render_report(&mut output, &*report)?;
        Ok(output)
    }
}

pub(crate) const fn terminal_color(setting: TerminalColor, terminal: bool) -> bool {
    match setting {
        TerminalColor::Auto => terminal,
        TerminalColor::Never => false,
        TerminalColor::Ansi => true,
    }
}

#[cfg(test)]
mod test_terminal_color {
    use crate::diagnostic::terminal_color;
    use ::sql_fun_core::TerminalColor;

    #[test]
    fn honors_explicit_mode_and_each_stream_terminal() {
        assert!(!terminal_color(TerminalColor::Auto, false));
        assert!(terminal_color(TerminalColor::Auto, true));
        assert!(!terminal_color(TerminalColor::Never, true));
        assert!(terminal_color(TerminalColor::Ansi, false));
    }
}

#[cfg(test)]
mod structured_fact_rendering_tests {
    use crate::{diagnostic::DiagnosticRenderer, highlighter_theme::HighlighterTheme};
    use ::sql_fun_core::analysis_problem::{
        AExprKind, AnalysisProblem, ArgumentBindingCollection, SemScalarExpr, TypeReference,
    };

    #[rstest::rstest]
    #[case::operator(
        AnalysisProblem::undefined_operator(
            AExprKind::new("operator"),
            "app.+",
            Some(SemScalarExpr::new("literal", Some(&TypeReference::new("pg_catalog.int4")), Some(true))),
            Some(SemScalarExpr::new("column reference", None, None)),
        ),
        "undefined_operator",
        &["app.+", "literal of type pg_catalog.int4", "column reference of undetermined type"]
    )]
    #[case::function_overload(
        AnalysisProblem::function_overload_resolution_failed(
            "public.compute",
            ArgumentBindingCollection::new(&[Some(TypeReference::new("pg_catalog.int4")), None]),
        ),
        "function_overload_resolution_failed",
        &["public.compute", "pg_catalog.int4", "undetermined type"]
    )]
    fn renders_readable_names_and_known_facts_without_tokens(
        #[case] problem: AnalysisProblem,
        #[case] code: &str,
        #[case] details: &[&str],
        #[values(None, Some("SELECT 1 OPERATOR(app.+) value, public.compute(1, value)"))]
        source: Option<&str>,
    ) -> testresult::TestResult {
        let renderer = DiagnosticRenderer {
            theme: HighlighterTheme::default(),
            color: false,
        };
        let output = renderer.render_problem(&problem, problem.severity(), source, None)?;
        let normalized = output.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(output.contains(code), "{output}");
        for detail in details {
            assert!(normalized.contains(*detail), "missing {detail}: {output}");
        }
        assert!(!output.contains("SemScalarExpr {"), "{output}");
        assert!(!output.contains("ArgumentBindingCollection("), "{output}");
        assert!(!output.contains("Some("), "{output}");
        assert!(!output.contains("\x1b["));
        Ok(())
    }
}

#[cfg(test)]
mod test_ansi_rendering {
    use crate::test_helpers::heavy;

    use crate::{diagnostic::DiagnosticRenderer, highlighter_theme::HighlighterTheme};
    use ::sql_fun_core::{
        SqlDialect,
        analysis_problem::{AnalysisProblem, FullName, StringSpan},
    };

    #[tokio::test]
    #[ignore = "High CI disk usage from server builds"]
    async fn applies_selected_theme_to_retained_sql() -> testresult::TestResult {
        let source = "create domain example as missing;";
        let problem = AnalysisProblem::domain_base_type_not_found(
            FullName::new("example"),
            "missing",
            StringSpan::new(source.find("missing").unwrap(), "missing".len()),
        );
        let renderer = DiagnosticRenderer {
            theme: HighlighterTheme::from_toml_str(r#"keyword = { foreground = "blue" }"#)?,
            color: true,
        };
        let tokens = heavy::tokenize_sql(source, SqlDialect::PostgreSQL).await?;
        let output = renderer.render(&problem, &tokens)?;
        assert!(output.contains("\x1b[34mcreate"));
        assert!(output.contains("domain_base_type_not_found"));
        Ok(())
    }
}

#[cfg(test)]
mod test_min_max_rendering {
    use crate::test_helpers::heavy;

    use crate::{diagnostic::DiagnosticRenderer, highlighter_theme::HighlighterTheme};
    use ::sql_fun_core::{
        SqlDialect,
        analysis_problem::{AnalysisProblem, MinMaxProblem},
    };

    #[rstest::rstest]
    #[case::empty(
        MinMaxProblem::empty(),
        "min_max_empty",
        "requires at least one argument"
    )]
    #[case::incompatible(
        MinMaxProblem::incompatible("pg_catalog.int4", "pg_catalog.text"),
        "min_max_incompatible_types",
        "cannot match pg_catalog.int4 and pg_catalog.text"
    )]
    #[case::unordered(
        MinMaxProblem::not_ordered("pg_catalog.json"),
        "min_max_not_ordered",
        "cannot identify a comparison function for pg_catalog.json"
    )]
    #[tokio::test]
    #[ignore = "High CI disk usage from server builds"]
    async fn preserves_min_max_message_and_code(
        #[case] problem: MinMaxProblem,
        #[case] code: &str,
        #[case] message: &str,
    ) -> testresult::TestResult {
        let renderer = DiagnosticRenderer {
            theme: HighlighterTheme::default(),
            color: false,
        };
        let tokens = heavy::tokenize_sql("select greatest(1, 2)", SqlDialect::PostgreSQL).await?;
        let output = renderer.render(&AnalysisProblem::min_max(problem), &tokens)?;
        assert!(output.contains(code));
        let normalized = output.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(normalized.contains(message));
        Ok(())
    }
}

#[cfg(test)]
mod test_implemented_family_rendering {
    use crate::test_helpers::heavy;

    use crate::{diagnostic::DiagnosticRenderer, highlighter_theme::HighlighterTheme};
    use ::sql_fun_core::{
        SqlDialect,
        analysis_problem::{
            AlterColumnProblem, AnalysisProblem, CallArgumentProblem, DeleteProblemKind,
            DiagnosticMessage, DiagnosticSeverity, IndexProblem, IndexProblemKind,
            InsertProblemKind, QueryProblem, RuleProblem, TriggerProblem, UpdateProblemKind,
            ViewProblem,
        },
    };

    #[rstest::rstest]
    #[case::query(
        AnalysisProblem::query(QueryProblem::duplicate_window_definition("named_window")),
        "query_duplicate_window_definition",
        "duplicate window definition: named_window",
        false
    )]
    #[case::inference_candidate_space(
        AnalysisProblem::inference_candidate_space_unsupported("comparison"),
        "inference_candidate_space_unsupported",
        "catalog-wide candidate search",
        false
    )]
    #[case::parameter_constraint(
        AnalysisProblem::parameter_constraint("$1 has conflicting requirements"),
        "parameter_constraint",
        "$1 has conflicting requirements",
        false
    )]
    #[case::insert(
        AnalysisProblem::insert(InsertProblemKind::arity(), "INSERT source has too many columns"),
        "insert_semantics",
        "INSERT source has too many columns",
        false
    )]
    #[case::update(
        AnalysisProblem::update(UpdateProblemKind::duplicate_target(), "column id assigned twice"),
        "update_semantics",
        "column id assigned twice",
        false
    )]
    #[case::delete(
        AnalysisProblem::delete(DeleteProblemKind::predicate()),
        "delete_semantics",
        "DELETE WHERE requires boolean type",
        false
    )]
    #[case::view(
        AnalysisProblem::view(&ViewProblem::unknown_default_column("public.summary", "missing")),
        "view_unknown_default_column", "default column missing does not exist on public.summary", false
    )]
    #[case::index_error(
        AnalysisProblem::index(&IndexProblem::target("public.items_idx", IndexProblemKind::missing_target())),
        "index_invalid_alteration", "public.items_idx: index does not exist", false
    )]
    #[case::index_warning(
        AnalysisProblem::index(&IndexProblem::statistics("public.idx", IndexProblemKind::statistics_clamped(), 1, 10001)),
        "index_statistics_clamped", "public.idx: lowering statistics target to 10000 (column 1): 10001", true
    )]
    #[case::alter_column_error(
        AnalysisProblem::alter_column("public.items", "id", &AlterColumnProblem::primary_key()),
        "alter_column_primary_key", "public.items.id: a primary-key column must remain NOT NULL", false
    )]
    #[case::alter_column_warning(
        AnalysisProblem::alter_column("public.items", "id", &AlterColumnProblem::statistics_reduced(10001)),
        "alter_column_statistics_reduced", "public.items.id: statistics target 10001 is reduced to 10000", true
    )]
    #[case::call_argument(
        AnalysisProblem::call_argument(CallArgumentProblem::duplicate_name("input", &None)),
        "call_argument_duplicate_name", r#"argument name "input" used more than once"#, false
    )]
    #[case::ownership(
        AnalysisProblem::ownership_role_not_found("missing_owner"),
        "ownership_role_not_found",
        "owner role does not exist: missing_owner",
        false
    )]
    #[case::rule(
        AnalysisProblem::rule(&RuleProblem::target("missing")),
        "rule_invalid_definition", "rule target missing does not resolve to a supported relation", false
    )]
    #[case::trigger(
        AnalysisProblem::trigger(&TriggerProblem::target("missing")),
        "trigger_invalid_definition", "trigger target missing does not resolve to a supported relation", false
    )]
    #[tokio::test]
    #[ignore = "High CI disk usage from server builds"]
    async fn preserves_implemented_family_presentation(
        #[case] problem: AnalysisProblem,
        #[case] code: &str,
        #[case] message: &str,
        #[case] warning: bool,
    ) -> testresult::TestResult {
        let renderer = DiagnosticRenderer {
            theme: HighlighterTheme::default(),
            color: false,
        };
        let tokens = heavy::tokenize_sql("select 1", SqlDialect::PostgreSQL).await?;
        let metadata = problem.render_metadata().expect("existing renderer");
        assert_eq!(metadata.message(), message);
        assert_eq!(problem.code(), code);
        let (severity, marker) = if warning {
            (DiagnosticSeverity::Warning, "⚠")
        } else {
            (DiagnosticSeverity::Error, "×")
        };
        assert_eq!(problem.severity(), severity);
        let output = renderer.render(&problem, &tokens)?;
        assert!(output.contains(code));
        assert!(output.contains(marker));
        let normalized = output.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            normalized.contains(&problem.diagnostic_message()),
            "{output}"
        );
        assert!(!output.contains("\x1b["));
        Ok(())
    }
}

/// Fetch and release tokens on the supplied connection, then synchronously render
/// a transported diagnostic using its retained SQL and CLI presentation settings.
///
/// # Errors
/// Returns an error if tokenization, theme loading, or rendering fails.
pub async fn render_diagnostic(
    client: &mut ::sql_fun_client::TcpJsonRpcClientAsync,
    presentation: &::sql_fun_core::DiagnosticPresentation,
    args: &SqlFunArgs,
) -> Result<String, anyhow::Error> {
    let tokens = client
        .tokenize_sql(presentation.source(), presentation.dialect())
        .await?;
    DiagnosticRenderer::from_args(args)?.render(presentation.problem(), &tokens)
}

#[cfg(test)]
mod test_rendering {
    use crate::test_helpers::heavy;

    use crate::diagnostic::render_diagnostic;
    use ::clap::Parser;
    use ::sql_fun_core::{
        DiagnosticPresentation, SqlDialect, SqlFunArgs,
        analysis_problem::{AnalysisProblem, FullName, StringSpan},
    };

    #[tokio::test]
    #[ignore = "High CI disk usage from server builds"]
    async fn renders_transported_diagnostic_with_original_metadata_and_label()
    -> testresult::TestResult {
        let source = "create domain example as missing;";
        let problem = AnalysisProblem::domain_base_type_not_found(
            FullName::new("example"),
            "missing",
            StringSpan::new(source.find("missing").unwrap(), "missing".len()),
        );
        let presentation =
            DiagnosticPresentation::new(&problem, source, SqlDialect::PostgreSQL, 41);
        let wire = ::serde_json::to_string(&presentation)?;
        let received = ::serde_json::from_str(&wire)?;
        let args = SqlFunArgs::try_parse_from(["sql-fun", "--term-color", "never"])?;
        let mut session = heavy::DiagnosticSession::new().await?;
        let output = render_diagnostic(session.client(), &received, &args).await?;
        assert!(output.contains("domain_base_type_not_found"));
        assert!(output.contains("domain base type not found: missing"));
        assert!(output.contains("base type name"));
        assert!(output.contains("missing"));
        assert!(output.contains(source));
        assert!(!output.contains("\x1b["));
        Ok(())
    }
}
