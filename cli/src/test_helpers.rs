use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};

/// Create a duplex stream pair for async IO tests.
pub fn make_duplex_stream(buffer_size: usize) -> (DuplexStream, DuplexStream) {
    tokio::io::duplex(buffer_size)
}

/// Write lines into an async writer and shut it down.
pub async fn write_lines<W>(writer: &mut W, lines: &[&str]) -> Result<(), std::io::Error>
where
    W: AsyncWrite + Unpin,
{
    for line in lines {
        writer.write_all(line.as_bytes()).await?;
    }
    writer.shutdown().await
}
