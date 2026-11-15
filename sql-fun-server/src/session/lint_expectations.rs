//! Owns a registration's immutable rules and mutable deprecated-diagnostic observations.
use crate::InitializedSchema;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use ::sql_fun_core::{
    LintConfigurations,
    analysis_problem::{AnalysisProblem, LintProblem, LintTarget},
};
use ::sql_fun_server_api::{
    SqlFunServerApiError,
    payloads::{AnalysisProbrem, DiagnosticSeverity},
};
use ::sql_fun_sqlast::{
    context::{BaseParseContext, SchemaFileContext},
    sem::DeprecatedObservations,
};

pub(super) struct LintExpectations {
    configuration: Arc<LintConfigurations>,
    path: Option<PathBuf>,
    expected: Vec<(usize, LintTarget)>,
    occurred: Vec<bool>,
}

impl LintExpectations {
    pub(super) fn new(
        schema: &InitializedSchema,
        configuration: &LintConfigurations,
        path: Option<&Path>,
    ) -> Result<(Self, Vec<AnalysisProbrem>), SqlFunServerApiError> {
        let mut state = Self {
            configuration: Arc::new(configuration.clone()),
            path: path.map(Path::to_path_buf),
            expected: Vec::new(),
            occurred: vec![false; configuration.deprecated().len()],
        };
        if !schema.has_project_schema() {
            return Ok((state, Vec::new()));
        }
        let base = schema.base_context();
        // These are configuration diagnostics, never SQL reference locations.
        let mut context =
            SchemaFileContext::new(state.source_path(), base, base.get_args(), base.metadata());
        context.set_lint_configuration(configuration);
        let observations = DeprecatedObservations::from_context(&mut context)
            .map_err(SqlFunServerApiError::new_schema_context_initialize_failed)?;
        for observation in observations
            .entries()
            .iter()
            .filter(|entry| entry.expected())
        {
            let item = &configuration.deprecated()[observation.index()];
            state.expected.push((
                observation.index(),
                LintTarget::new(item.kind(), item.name(), item.column(), item.label()),
            ));
        }
        let warnings = context
            .problems()
            .iter()
            .map(|problem| state.diagnostic(problem))
            .collect();
        Ok((state, warnings))
    }

    pub(super) fn configuration(&self) -> Arc<LintConfigurations> {
        Arc::clone(&self.configuration)
    }

    pub(super) fn observe(&mut self, observations: &DeprecatedObservations) {
        for observation in observations.entries() {
            self.occurred[observation.index()] |= observation.occurred();
        }
    }

    pub(super) fn report(&self) -> Vec<AnalysisProbrem> {
        self.expected
            .iter()
            .filter(|(index, _)| !self.occurred[*index])
            .map(|(_, target)| {
                self.diagnostic(&AnalysisProblem::lint(&LintProblem::unmet_expectation(
                    target,
                )))
            })
            .collect()
    }

    fn source_path(&self) -> &Path {
        self.path
            .as_deref()
            .unwrap_or_else(|| Path::new("<lint configuration>"))
    }

    fn diagnostic(&self, problem: &AnalysisProblem) -> AnalysisProbrem {
        AnalysisProbrem::new(
            problem.code(),
            DiagnosticSeverity::from(problem.severity()),
            self.source_path(),
            None,
        )
        .with_semantic_problem(problem)
    }
}

// Integration coverage exercises registration, SQLAST handoff, dispatch, and collection ownership.
#[cfg(test)]
mod test_reporting {
    use crate::{
        Api,
        session::{SessionState, SessionWrapper, facade::Session},
        test_helpers::lint_expectations::{LintProject, rpc, rpc_failed, rule},
    };
    use std::{num::NonZeroUsize, sync::OnceLock};

    use ::serde_json::json;

    #[tokio::test]
    async fn reports_are_snapshots_and_analysis_continues_after_reporting() {
        let project = LintProject::new(&rule(r#"["retained"]"#, "expect = true"));
        let api = Api {
            schema_cache: OnceLock::new(),
        };
        let wrapper = SessionWrapper::new();
        let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let mut session = Session::new(&wrapper.contexts, &mut state);
        let initialized = session
            .process_request(&api, &rpc("initialize_schema_context", project.request()))
            .await
            .unwrap_or_else(rpc_failed);
        let selection = json!({"context_handle":initialized["context_handle"]});
        let first = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        let second = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(first["problems"]["items"], second["problems"]["items"]);
        assert_ne!(first["problems"]["handle"], second["problems"]["handle"]);
        let problem = &first["problems"]["items"][0];
        assert_eq!(problem["code"], "unmet_lint_expectation");
        assert_eq!(problem["severity"], "warning");
        assert_eq!(problem["file_path"], project.lint_path().to_str().unwrap());
        assert!(problem["semantic_problem"].is_object());
        assert!(problem["statement_span"].is_null());
        assert!(problem["probrem_span"].is_null());
        assert!(problem["statement_relative_span"].is_null());
        let decoded: ::sql_fun_server_api::payloads::ReportLintExpectationsResponse =
            ::serde_json::from_value(first.clone()).unwrap();
        assert_eq!(::serde_json::to_value(decoded).unwrap(), first);

        let query = json!({"context_handle":selection["context_handle"], "query":{"Text":"SELECT id FROM retained; SELECT FROM;"}});
        let analyzed = session
            .process_request(&api, &rpc("analyze_query", &query))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(
            analyzed["queries"]["items"][0]["probrems"]["items"][0]["code"],
            "deprecated"
        );
        let third = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(third["problems"]["total_length"], 0);
        // Previously returned reports retain their original diagnostics after analysis.
        let read = json!({"collection_handle":first["problems"]["handle"], "start_offset":0});
        let old = session
            .process_request(&api, &rpc("read_collection", &read))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(old, first["problems"]);
        session
            .process_request(
                &api,
                &rpc(
                    "release_collection",
                    &json!({"collection_handle":first["problems"]["handle"]}),
                ),
            )
            .await
            .unwrap_or_else(rpc_failed);
        let still_satisfied = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(still_satisfied["problems"]["total_length"], 0);
        session
            .process_request(&api, &rpc("analyze_query", &query))
            .await
            .unwrap_or_else(rpc_failed);
        let second_read =
            json!({"collection_handle":second["problems"]["handle"], "start_offset":0});
        assert_eq!(
            session
                .process_request(&api, &rpc("read_collection", &second_read))
                .await
                .unwrap_or_else(rpc_failed),
            second["problems"]
        );
    }

    #[rstest::rstest]
    #[case::empty("")]
    #[case::unrelated("SELECT 1")]
    #[case::syntax_failure("SELECT FROM;")]
    #[tokio::test]
    async fn missing_observations_do_not_cancel_expectations(#[case] sql: &str) {
        let project = LintProject::new(&rule(r#"["retained"]"#, "expect = true"));
        let api = Api {
            schema_cache: OnceLock::new(),
        };
        let wrapper = SessionWrapper::new();
        let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let mut session = Session::new(&wrapper.contexts, &mut state);
        let initialized = session
            .process_request(&api, &rpc("initialize_schema_context", project.request()))
            .await
            .unwrap_or_else(rpc_failed);
        let query = json!({"context_handle":initialized["context_handle"], "query":{"Text":sql}});
        session
            .process_request(&api, &rpc("analyze_query", &query))
            .await
            .unwrap_or_else(rpc_failed);
        let reported = session
            .process_request(&api, &rpc("report_lint_expectations", &query))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(reported["problems"]["total_length"], 1);
    }

    #[rstest::rstest]
    #[case::true_first("expect = true", "expect = false", 1)]
    #[case::true_second("expect = false", "expect = true", 1)]
    #[case::omitted_first("", "expect = true", 1)]
    #[case::omitted_second("expect = true", "", 1)]
    #[case::no_expectation("expect = false", "", 0)]
    #[tokio::test]
    async fn duplicate_rules_keep_one_effective_expectation(
        #[case] first: &str,
        #[case] second: &str,
        #[case] expected: usize,
    ) {
        let declarations =
            rule(r#"["retained"]"#, first) + &rule(r#"["public", "retained"]"#, second);
        let project = LintProject::new(&declarations);
        let api = Api {
            schema_cache: OnceLock::new(),
        };
        let wrapper = SessionWrapper::new();
        let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let mut session = Session::new(&wrapper.contexts, &mut state);
        let initialized = session
            .process_request(&api, &rpc("initialize_schema_context", project.request()))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(
            initialized["problems"]["items"][0]["code"],
            "duplicate_lint_target"
        );
        let selection = json!({"context_handle":initialized["context_handle"]});
        let before = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(before["problems"]["total_length"], expected);
        let query = json!({"context_handle":selection["context_handle"], "query":{"Text":"SELECT id FROM retained"}});
        session
            .process_request(&api, &rpc("analyze_query", &query))
            .await
            .unwrap_or_else(rpc_failed);
        let after = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(after["problems"]["total_length"], 0);
    }

    #[tokio::test]
    async fn unresolved_expectations_are_bounded_and_configuration_warnings_do_not_satisfy_them() {
        let declarations = rule(r#"["missing"]"#, "expect = true")
            + &rule(r#"["retained"]"#, "column = 'missing'\nexpect = true")
            + &rule(r#"["retained"]"#, "label = 'invalid'\nexpect = true");
        let project = LintProject::new(&declarations);
        let api = Api {
            schema_cache: OnceLock::new(),
        };
        let wrapper = SessionWrapper::new();
        let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let mut session = Session::new(&wrapper.contexts, &mut state);
        let initialized = session
            .process_request(&api, &rpc("initialize_schema_context", project.request()))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(initialized["problems"]["total_length"], 3);
        assert_eq!(
            initialized["problems"]["items"].as_array().unwrap().len(),
            2
        );
        assert_eq!(
            initialized["problems"]["items"][0]["code"],
            "lint_configuration"
        );
        let selection = json!({"context_handle":initialized["context_handle"]});
        let report = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        let page = &report["problems"];
        assert_eq!(page["total_length"], 3);
        assert_eq!(page["items"].as_array().unwrap().len(), 2);
        let last = session
            .process_request(
                &api,
                &rpc(
                    "read_collection",
                    &json!({"collection_handle":page["handle"], "start_offset":2}),
                ),
            )
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(last["items"].as_array().unwrap().len(), 1);
        assert_eq!(last["items"][0]["code"], "unmet_lint_expectation");
        session
            .process_request(
                &api,
                &rpc(
                    "release_collection",
                    &json!({"collection_handle":page["handle"]}),
                ),
            )
            .await
            .unwrap_or_else(rpc_failed);
        let again = session
            .process_request(&api, &rpc("report_lint_expectations", &selection))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(again["problems"]["total_length"], 3);
    }

    #[tokio::test]
    async fn handles_share_schema_but_keep_configuration_and_observations_independent() {
        let project = LintProject::new(&rule(r#"["retained"]"#, "expect = true"));
        let api = Api {
            schema_cache: OnceLock::new(),
        };
        let wrapper = SessionWrapper::new();
        let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let mut session = Session::new(&wrapper.contexts, &mut state);
        let first = session
            .process_request(&api, &rpc("initialize_schema_context", project.request()))
            .await
            .unwrap_or_else(rpc_failed);
        let second = session
            .process_request(&api, &rpc("initialize_schema_context", project.request()))
            .await
            .unwrap_or_else(rpc_failed);
        let first_handle = ::serde_json::from_value(first["context_handle"].clone()).unwrap();
        let second_handle = ::serde_json::from_value(second["context_handle"].clone()).unwrap();
        assert!(std::ptr::eq(
            session.context(&first_handle).unwrap().analysis(),
            session.context(&second_handle).unwrap().analysis()
        ));
        // Editing/removing the selected file does not change an existing handle's configuration.
        project.configure("");
        let third = session
            .process_request(&api, &rpc("initialize_schema_context", project.request()))
            .await
            .unwrap_or_else(rpc_failed);
        std::fs::remove_file(project.lint_path()).unwrap();
        let query = json!({"context_handle":first["context_handle"], "query":{"Text":"SELECT id FROM retained"}});
        session
            .process_request(&api, &rpc("analyze_query", &query))
            .await
            .unwrap_or_else(rpc_failed);
        let one = session
            .process_request(&api, &rpc("report_lint_expectations", &first))
            .await
            .unwrap_or_else(rpc_failed);
        let two = session
            .process_request(&api, &rpc("report_lint_expectations", &second))
            .await
            .unwrap_or_else(rpc_failed);
        let three = session
            .process_request(&api, &rpc("report_lint_expectations", &third))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(one["problems"]["total_length"], 0);
        assert_eq!(two["problems"]["total_length"], 1);
        assert_eq!(three["problems"]["total_length"], 0);
    }

    #[rstest::rstest]
    #[case::builtin_prefix("builtin", false)]
    #[case::unconfigured_project("project", true)]
    #[tokio::test]
    async fn contexts_without_project_rules_report_empty(
        #[case] level: &str,
        #[case] omit_configuration: bool,
    ) {
        let project = LintProject::new("");
        std::fs::remove_file(project.lint_path()).unwrap();
        let mut request = project.request().clone();
        request["schema_level"] = json!(level);
        if omit_configuration {
            project.omit_configuration();
        }
        let api = Api {
            schema_cache: OnceLock::new(),
        };
        let wrapper = SessionWrapper::new();
        let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let mut session = Session::new(&wrapper.contexts, &mut state);
        let initialized = session
            .process_request(&api, &rpc("initialize_schema_context", &request))
            .await
            .unwrap_or_else(rpc_failed);
        let report = session
            .process_request(&api, &rpc("report_lint_expectations", &initialized))
            .await
            .unwrap_or_else(rpc_failed);
        assert_eq!(report["problems"]["total_length"], 0);
    }
}
