#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

use crate::{
    DiagnosticRenderer,
    analysis::{PreparedDiagnostics, initialization},
};

use std::collections::{HashMap, hash_map::Entry};

use crate::analysis::{prepare_tokens, request_typed};
use ::anyhow::Context;
use ::sql_fun_client::{
    TcpJsonRpcClientAsync,
    payloads::{AnalyzeQueryArgs, AnalyzeQueryResponse, ContextHandle, SchemaContext},
};
use ::sql_fun_codegen::QueryRequirement;
use ::sql_fun_core::{PlaceholderedSql, SqlFunArgs};
use ::tokio::sync::mpsc::Sender;

/// CLI-owned source identity paired with a fully materialized server response.
/// Original SQL and the core catalog fingerprint are captured before transfer.
pub(super) struct QueryAnalysis {
    function_name: String,
    original_query: String,
    query_finger_print: String,
    source_file: String,
    source_line: u32,
    datasource: String,
    response: AnalyzeQueryResponse,
    tokenizations: PreparedDiagnostics,
}

impl QueryAnalysis {
    pub(super) fn function_name(&self) -> &str {
        &self.function_name
    }
    pub(super) fn original_query(&self) -> &str {
        &self.original_query
    }
    pub(super) fn query_finger_print(&self) -> &str {
        &self.query_finger_print
    }
    pub(super) fn source_file(&self) -> &str {
        &self.source_file
    }
    pub(super) const fn source_line(&self) -> u32 {
        self.source_line
    }
    pub(super) fn datasource(&self) -> &str {
        &self.datasource
    }
    pub(super) const fn response(&self) -> &AnalyzeQueryResponse {
        &self.response
    }

    pub(super) fn render(
        &self,
        diagnostic: &::sql_fun_client::payloads::AnalysisProbrem,
        renderer: &DiagnosticRenderer,
    ) -> Result<String, ::anyhow::Error> {
        self.tokenizations.render(diagnostic, renderer)
    }

    pub(super) fn tokenization_error(&self) -> Option<&str> {
        self.tokenizations.error()
    }

    pub(super) fn log_received(&self) {
        ::log::debug!(
            "received {} query analyses for {} at {}:{} (datasource {}, fingerprint {}, original SQL bytes {})",
            self.response().queries().items().len(),
            self.function_name(),
            self.source_file(),
            self.source_line(),
            self.datasource(),
            self.query_finger_print(),
            self.original_query().len(),
        );
    }
}

/// Deliver each materialized response with its extracted identity, applying channel
/// backpressure before submitting the next query. Dropping the receiver is an error.
/// The owned sender closes on return; EOF alone is not proof of success. The caller
/// must also check this result and the client's completion result.
/// A single connection owns all context handles; reuse only within this batch.
pub(super) async fn submit(
    client: &mut TcpJsonRpcClientAsync,
    args: &SqlFunArgs,
    requirements: Vec<QueryRequirement>,
    results: Sender<QueryAnalysis>,
) -> Result<(), ::anyhow::Error> {
    let mut contexts = HashMap::<String, ContextHandle>::new();
    for requirement in requirements {
        let datasource = requirement.datasource().to_owned();
        let function_name = requirement.function_name().to_owned();
        let source_file = requirement.source_file().to_owned();
        let source_line = requirement.source_line();
        let original_query = requirement.query().to_owned();
        let query_finger_print = requirement
            .cte_catalog()
            .finger_print(&PlaceholderedSql::from_query(requirement.query()))?;
        if let Entry::Vacant(entry) = contexts.entry(datasource.clone()) {
            let request =
                initialization(args, &datasource, ::sql_fun_core::SchemaLevel::project())?;
            let initialized: SchemaContext =
                request_typed(client, "initialize_schema_context", &request)
                    .await
                    .with_context(|| format!("cannot initialize datasource {datasource}"))?;
            let handle = initialized.context_handle().as_ref().with_context(|| {
                format!("datasource {datasource} returned no usable context handle")
            })?;
            entry.insert(handle.clone());
        }
        let request = AnalyzeQueryArgs::with_catalog(
            &contexts[&datasource],
            &original_query,
            requirement.into_cte_catalog(),
        )
        .with_access_analysis();
        let response = request_typed(client, "analyze_query", &request)
            .await
            .with_context(|| format!("cannot analyze {function_name} at {source_file}:{source_line} (datasource {datasource})"))?;
        let tokenizations = prepare_tokens(client, &response).await;
        let analysis = QueryAnalysis {
            function_name,
            original_query,
            query_finger_print,
            source_file,
            source_line,
            datasource,
            response,
            tokenizations,
        };
        results
            .send(analysis)
            .await
            .map_err(|_| ::anyhow::anyhow!("analysis result receiver closed before delivery"))?;
    }
    Ok(())
}
