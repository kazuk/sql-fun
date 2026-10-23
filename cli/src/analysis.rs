use ::sql_fun_client::{
    TcpJsonRpcClientAsync,
    payloads::{AnalysisProbrem, AnalyzeQueryResponse, InitializeSchemaArgs},
};
use ::sql_fun_core::{Hasher, SchemaLevel, SqlDialect, SqlFunArgs, TokenStream};

#[derive(Default)]
pub(crate) struct PreparedDiagnostics {
    entries: Vec<PreparedTokenization>,
}
struct PreparedTokenization {
    source: String,
    dialect: SqlDialect,
    result: Result<TokenStream, String>,
}
impl PreparedDiagnostics {
    pub(crate) fn tokens(&self, source: &str, dialect: SqlDialect) -> Option<&TokenStream> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.source == source && entry.dialect == dialect)?;
        entry.result.as_ref().ok()
    }
    pub(crate) fn error(&self) -> Option<&str> {
        self.entries
            .iter()
            .find_map(|entry| entry.result.as_ref().err().map(String::as_str))
    }
    pub(crate) fn render(
        &self,
        diagnostic: &AnalysisProbrem,
        renderer: &crate::diagnostic::DiagnosticRenderer,
    ) -> Result<String, ::anyhow::Error> {
        if let Some(presentation) = diagnostic.presentation() {
            renderer.render_problem(
                presentation.problem(),
                (*diagnostic.severity()).into(),
                Some(presentation.source()),
                self.tokens(presentation.source(), presentation.dialect()),
            )
        } else if let Some(problem) = diagnostic.semantic_problem() {
            renderer.render_problem(problem, (*diagnostic.severity()).into(), None, None)
        } else {
            ::anyhow::bail!(
                "analysis diagnostic {} lacks a structured problem",
                diagnostic.code()
            )
        }
    }
}

pub(crate) async fn prepare_diagnostics(
    client: &mut TcpJsonRpcClientAsync,
    diagnostics: &[AnalysisProbrem],
) -> PreparedDiagnostics {
    let mut prepared = PreparedDiagnostics::default();
    prepared.prepare(client, diagnostics).await;
    prepared
}

impl PreparedDiagnostics {
    async fn prepare(
        &mut self,
        client: &mut TcpJsonRpcClientAsync,
        diagnostics: &[AnalysisProbrem],
    ) {
        for diagnostic in diagnostics {
            let Some(presentation) = diagnostic.presentation() else {
                continue;
            };
            if self.entries.iter().any(|entry| {
                entry.source == presentation.source() && entry.dialect == presentation.dialect()
            }) {
                continue;
            }
            let result = client
                .tokenize_sql(presentation.source(), presentation.dialect())
                .await
                .map_err(|error| error.to_string());
            self.entries.push(PreparedTokenization {
                source: presentation.source().to_owned(),
                dialect: presentation.dialect(),
                result,
            });
        }
    }
}

/// Lex each retained source/dialect once across the entire response.
pub(crate) async fn prepare_tokens(
    client: &mut TcpJsonRpcClientAsync,
    response: &AnalyzeQueryResponse,
) -> PreparedDiagnostics {
    let mut prepared = PreparedDiagnostics::default();
    for item in response.queries().items() {
        prepared.prepare(client, item.probrems().items()).await;
    }
    prepared
}

pub(crate) fn initialization(
    args: &SqlFunArgs,
    datasource: &str,
    level: SchemaLevel,
) -> Result<InitializeSchemaArgs, ::anyhow::Error> {
    let selected = args.with_datasource(datasource);
    let environments = selected.analysis_environments(level)?;
    let dump = if level.includes_project() {
        Some(std::path::absolute(
            selected.schema_file(&selected.metadata_file()?)?,
        )?)
    } else {
        None
    };
    let hash = dump.as_ref().map(Hasher::hash_file).transpose()?;
    Ok(InitializeSchemaArgs::for_level(
        datasource,
        dump.as_deref(),
        hash.as_deref(),
        &environments,
        level,
    ))
}

/// Await the existing typed client's completion, including paging and releases.
pub(crate) async fn request_typed<R>(
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
