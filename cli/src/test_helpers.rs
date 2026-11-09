pub(crate) mod direct_sql;
pub(crate) mod heavy;
pub(crate) mod installation;
pub(crate) mod inventory;

use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};

/// Create a duplex stream pair for async IO tests.
#[must_use]
pub fn make_duplex_stream(buffer_size: usize) -> (DuplexStream, DuplexStream) {
    tokio::io::duplex(buffer_size)
}

/// Write lines into an async writer and shut it down.
///
/// # Errors
///
/// Returns an I/O error if writing or shutting down the writer fails.
pub async fn write_lines<W>(writer: &mut W, lines: &[&str]) -> Result<(), std::io::Error>
where
    W: AsyncWrite + Unpin,
{
    for line in lines {
        writer.write_all(line.as_bytes()).await?;
    }
    writer.shutdown().await
}

pub(crate) mod theme;

/// Encode catalog values using psql's NUL record separator.
pub(crate) fn catalog_results(values: &[serde_json::Value]) -> String {
    let mut output = String::new();
    for value in values {
        output.push_str(&value.to_string());
        output.push('\0');
    }
    output
}

/// Frame the version, scope check, and catalog payloads as a psql response.
pub(crate) fn extraction_results(
    version: u32,
    schemas_exist: bool,
    catalogs: &[serde_json::Value],
) -> String {
    let mut values = vec![serde_json::json!(version), serde_json::json!(schemas_exist)];
    values.extend_from_slice(catalogs);
    catalog_results(&values)
}
