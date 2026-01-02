#![allow(dead_code)]

mod pid_file;

use clap::Parser;
use pid_file::PidFile;
use std::net::SocketAddr;
use tokio::{io::AsyncBufReadExt, net::TcpStream};

#[derive(clap::Parser, Clone)]
pub struct Args {
    #[arg(short, long, env = "USER")]
    user: String,
}

/// Run the server event loop until interrupted.
///
/// # Errors
///
/// Returns [`ServerError`] when binding or accepting sockets fails.
pub async fn service() -> Result<(), ServerError> {
    let args = Args::parse();

    let listner = tokio::net::TcpListener::bind("127.0.0.1:0").await?;

    let local_addr = listner.local_addr()?;
    let port = local_addr.port();
    let Some(_pid_file) = PidFile::try_register_primary_server(&args, port)? else {
        return Ok(());
    };

    loop {
        let (accepted_socker, remote_addr) = listner.accept().await?;

        tokio::task::spawn(async move { serve_client(accepted_socker, remote_addr).await });
    }
}

#[derive(serde::Deserialize)]
struct JsonRpcRequest {}

async fn serve_client(stream: TcpStream, _addr: SocketAddr) -> Result<(), ServerError> {
    let (read_half, _write_half) = stream.into_split();
    let mut buf_read = tokio::io::BufReader::new(read_half);

    let mut line = String::new();
    buf_read.read_line(&mut line).await?;

    let _request_message: JsonRpcRequest = serde_json::from_str(&line)?;
    todo!();
}

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("logic bug {0}")]
    Bug(String),
}

struct ReadableBuffer {
    buffer_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    return_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    current_buffer: Option<Vec<u8>>,
    inner_bufpos: usize,
}

impl ReadableBuffer {
    fn fill_buff(&mut self) {
        if self.current_buffer.is_none() {
            self.inner_bufpos = 0;
            self.current_buffer = self.buffer_rx.blocking_recv();
        }
    }

    fn return_buff(&mut self) -> std::io::Result<()> {
        let Some(bufflen) = self.current_buffer.as_ref().map(Vec::len) else {
            return Ok(());
        };
        if bufflen == self.inner_bufpos {
            let mut buffer = self.current_buffer.take().unwrap();
            buffer.clear();
            if self.return_tx.blocking_send(buffer).is_err() {
                return Err(std::io::Error::other("sender closed"));
            }
            self.inner_bufpos = 0;
        }
        Ok(())
    }
}

impl std::io::Read for ReadableBuffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.fill_buff();

        let Some(current_buff) = &self.current_buffer else {
            return Ok(0);
        };
        let source_slice = &current_buff[self.inner_bufpos..];
        let len = buf.len().min(source_slice.len());

        buf[..len].copy_from_slice(&source_slice[..len]);
        self.inner_bufpos += len;
        self.return_buff()?;
        Ok(len)
    }
}

struct PipeBuffer {
    buffer_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    return_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

impl PipeBuffer {
    pub async fn pipe_read_until<A: AsyncBufReadExt + std::marker::Unpin>(
        &mut self,
        source: &mut A,
        byte: u8,
    ) -> Result<usize, std::io::Error> {
        let Some(mut buff) = self.return_rx.recv().await else {
            return Err(std::io::Error::other("receiver closed"))?;
        };
        buff.clear();
        let size = source.read_until(byte, &mut buff).await?;
        if size != 0 {
            self.buffer_tx
                .send(buff)
                .await
                .map_err(|_| std::io::Error::other("No receiver"))?;
        }
        Ok(size)
    }
}
