// Share the existing isolated real-server lifecycle fixture and guarded cleanup.
#[allow(dead_code)]
#[path = "../../../sql-fun-client/tests/test_helpers/heavy.rs"]
pub(crate) mod lifecycle;

/// Starts an actual isolated server and connects over the normal authenticated
/// transport. Each test owns its server; only Cargo's build cache is shared.
/// Rendering tests need this expensive setup to cover the tokenization boundary.
pub(crate) struct DiagnosticSession {
    _server: lifecycle::ServerFixture,
    client: ::sql_fun_client::TcpJsonRpcClientAsync,
}
impl DiagnosticSession {
    pub(crate) async fn new() -> ::testresult::TestResult<Self> {
        let server = lifecycle::ServerFixture::new()?;
        let client = ::sql_fun_client::connect(&server.options()).await?;
        Ok(Self {
            _server: server,
            client,
        })
    }
    pub(crate) fn client(&mut self) -> &mut ::sql_fun_client::TcpJsonRpcClientAsync {
        &mut self.client
    }
}

/// Fetches a real server's complete token collection and releases it before
/// returning. Starts one isolated server per call; no catalog is initialized.
pub(crate) async fn tokenize_sql(
    source: &str,
    dialect: ::sql_fun_core::SqlDialect,
) -> ::testresult::TestResult<::sql_fun_core::TokenStream> {
    let mut session = DiagnosticSession::new().await?;
    let tokens = session.client.tokenize_sql(source, dialect).await?;
    session.client.finish().await?;
    Ok(tokens)
}
