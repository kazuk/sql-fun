#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, hash_map::Entry};

use ::anyhow::Context;
use ::sql_fun_client::{
    TcpJsonRpcClientAsync,
    payloads::{
        AnalyzeQueryArgs, AnalyzeQueryResponse, ContextHandle, InitializeSchemaArgs, SchemaContext,
    },
};
use ::sql_fun_codegen::QueryRequirement;
use ::sql_fun_core::{Hasher, SqlFunArgs};

/// CLI-owned source identity paired with a fully materialized server response.
/// Diagnostic and lockfile consumers are connected separately under #689.
pub(super) struct QueryAnalysis {
    function_name: String,
    source_file: String,
    source_line: u32,
    datasource: String,
    response: AnalyzeQueryResponse,
}

impl QueryAnalysis {
    pub(super) fn log_received(&self) {
        ::log::debug!(
            "received {} query analyses for {} at {}:{} (datasource {})",
            self.response.queries().items().len(),
            self.function_name,
            self.source_file,
            self.source_line,
            self.datasource,
        );
    }
}

/// A single connection owns all context handles; reuse only within this batch.
pub(super) async fn submit(
    client: &mut TcpJsonRpcClientAsync,
    args: &SqlFunArgs,
    requirements: Vec<QueryRequirement>,
) -> Result<Vec<QueryAnalysis>, ::anyhow::Error> {
    let mut contexts = HashMap::<String, ContextHandle>::new();
    let mut analyses = Vec::new();
    for requirement in requirements {
        let datasource = requirement.datasource().to_owned();
        let function_name = requirement.function_name().to_owned();
        let source_file = requirement.source_file().to_owned();
        let source_line = requirement.source_line();
        if let Entry::Vacant(entry) = contexts.entry(datasource.clone()) {
            let request = initialization(args, &datasource)?;
            let initialized: SchemaContext =
                request_typed(client, "initialize_schema_context", &request)
                    .await
                    .with_context(|| format!("cannot initialize datasource {datasource}"))?;
            let handle = initialized.context_handle().as_ref().with_context(|| {
                format!("datasource {datasource} returned no usable context handle")
            })?;
            entry.insert(handle.clone());
        }
        let request =
            AnalyzeQueryArgs::from_catalog(&contexts[&datasource], requirement.into_cte_catalog());
        let response = request_typed(client, "analyze_query", &request)
            .await
            .with_context(|| format!("cannot analyze {function_name} at {source_file}:{source_line} (datasource {datasource})"))?;
        analyses.push(QueryAnalysis {
            function_name,
            source_file,
            source_line,
            datasource,
            response,
        });
    }
    Ok(analyses)
}

fn initialization(
    args: &SqlFunArgs,
    datasource: &str,
) -> Result<InitializeSchemaArgs, ::anyhow::Error> {
    let selected = args.with_datasource(datasource);
    let metadata_file = selected.metadata_file()?;
    let schema_dump = std::path::absolute(selected.schema_file(&metadata_file)?)?;
    let hash = Hasher::hash_file(&schema_dump)?;
    let environments = selected.environments()?;
    Ok(InitializeSchemaArgs::new(
        datasource,
        &schema_dump,
        &hash,
        &environments,
    ))
}

/// Await the existing typed client's completion, including paging and releases.
async fn request_typed<R>(
    client: &mut TcpJsonRpcClientAsync,
    method: &str,
    request: &impl ::serde::Serialize,
) -> Result<R, ::anyhow::Error>
where
    R: ::sql_fun_client::CompleteResponse + ::serde::de::DeserializeOwned + Send + 'static,
{
    let params = ::serde_json::to_value(request)?;
    let (sender, receiver) = ::tokio::sync::oneshot::channel();
    client
        .invoke_typed(method, &params, |result| async move {
            let _ = sender.send(result);
            Ok(())
        })
        .await?;
    Ok(receiver.await??)
}
