use std::{io::Read, ops::Range, sync::Arc, time::Duration};

use tokio::sync::{Mutex, mpsc::Permit};

pub struct BufferChunk {
    chunk: Box<[u8]>,
    filled: Range<usize>,
    delimited: Option<Range<usize>>,
}

type DelimitedRangeWithEof = (Option<Range<usize>>, bool);

impl BufferChunk {
    fn with_capacity(capacity: usize) -> Self {
        let vec = vec![0; capacity];
        Self {
            chunk: vec.into_boxed_slice(),
            filled: 0..0,
            delimited: None,
        }
    }

    const fn clear(&mut self) {
        self.filled = 0..0;
        self.delimited = None;
    }

    fn filled(&self) -> &[u8] {
        let slice = self.chunk.as_ref();
        &slice[self.filled.clone()]
    }

    async fn fill_from<A>(
        &mut self,
        source: &mut A,
        delimiter: u8,
        idle_timeout: Duration,
    ) -> Result<DelimitedRangeWithEof, tokio::io::Error>
    where
        A: tokio::io::AsyncRead + tokio::io::AsyncReadExt + std::marker::Unpin,
    {
        let current_end = self.filled.end;
        let size = tokio::time::timeout(idle_timeout, source.read(&mut self.chunk[current_end..]))
            .await
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "receive idle timeout")
            })??;
        let eof = size == 0;

        let read_slice = &self.chunk[current_end..current_end + size];
        if let Some(delimiter_pos) = read_slice.iter().position(|b| *b == delimiter) {
            self.delimited = Some(self.filled.start..current_end + delimiter_pos + 1);
        }
        self.filled.end += size;
        Ok((self.delimited.clone(), eof))
    }

    fn fill_exact(&mut self, slice: &[u8], delimiter: u8) {
        let fill_range = self.filled.end..self.filled.end + slice.len();

        self.chunk[fill_range.clone()].copy_from_slice(slice);
        if let Some(delimiter_pos) = self.chunk[fill_range.clone()]
            .iter()
            .position(|b| *b == delimiter)
        {
            self.delimited = Some(self.filled.start..fill_range.start + delimiter_pos + 1);
        }
        self.filled.end += slice.len();
    }

    fn carry_over(&mut self, next: &mut Self, keep: &Range<usize>, delimiter: u8) {
        let carry_slice = &self.chunk[keep.end..self.filled.end];
        next.fill_exact(carry_slice, delimiter);
        self.filled.end = keep.end;
    }

    fn delimited(&self) -> Option<Range<usize>> {
        self.delimited.clone()
    }

    fn is_full(&self) -> bool {
        self.filled.end == self.chunk.len()
    }

    fn take_filled(&mut self, buf: &mut [u8]) -> usize {
        let source_slice = self.filled();
        let len = buf.len().min(source_slice.len());
        buf[..len].copy_from_slice(&source_slice[..len]);
        self.filled.start += len;
        len
    }

    fn is_empty(&self) -> bool {
        self.filled.is_empty()
    }
}

/// JSON RPC TCP Stream buffer
pub struct ReadableBuffer {
    buffer_rx: tokio::sync::mpsc::Receiver<Result<BufferChunk, std::io::Error>>,
    return_tx: tokio::sync::mpsc::Sender<BufferChunk>,
    current_buffer: Option<Result<BufferChunk, std::io::Error>>,
}

impl ReadableBuffer {
    fn fill_buff(&mut self) {
        if self.current_buffer.is_none() {
            tokio::task::block_in_place(|| {
                self.current_buffer = self.buffer_rx.blocking_recv();
            });
        }
    }

    fn return_buff(&mut self, mut buffer: BufferChunk) -> std::io::Result<()> {
        buffer.clear();
        let send_result = tokio::task::block_in_place(|| self.return_tx.blocking_send(buffer));
        if send_result.is_err() {
            return Err(std::io::Error::other("sender closed"));
        }
        Ok(())
    }
}

impl std::io::Read for ReadableBuffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.fill_buff();

        let Some(current_buff) = self.current_buffer.take() else {
            return Ok(0);
        };
        match current_buff {
            Ok(mut current_buff) => {
                let len = current_buff.take_filled(buf);
                if current_buff.is_empty() {
                    self.return_buff(current_buff)?;
                } else {
                    self.current_buffer = Some(Ok(current_buff));
                }
                Ok(len)
            }
            Err(e) => Err(e),
        }
    }
}

/// Pipe buffer
pub struct PipeBuffer<Source>
where
    Self: Send,
    Source: tokio::io::AsyncRead + tokio::io::AsyncReadExt + std::marker::Unpin + Send,
{
    buffer_tx: tokio::sync::mpsc::Sender<Result<BufferChunk, std::io::Error>>,
    return_rx: tokio::sync::mpsc::Receiver<BufferChunk>,
    current_buff: Option<BufferChunk>,
    delimiter: u8,
    source: Source,
    limit_length: usize,
    request_length: usize,
    idle_timeout: Duration,

    next_buff: Option<BufferChunk>,
    fill_result: Option<Result<DelimitedRangeWithEof, tokio::io::Error>>,
    eof: bool,
}

impl<Source> PipeBuffer<Source>
where
    Source: tokio::io::AsyncRead + tokio::io::AsyncReadExt + std::marker::Unpin + Send + Sync,
{
    async fn reserve_next_buff(&mut self) -> Result<(), std::io::Error> {
        if self.next_buff.is_some() {
            return Ok(());
        }

        let Some(mut buff) = self.return_rx.recv().await else {
            return Err(std::io::Error::other("receiver closed"))?;
        };
        buff.clear();
        self.next_buff = Some(buff);
        Ok(())
    }

    fn send_buffer(&mut self, permit: Permit<'_, Result<BufferChunk, std::io::Error>>) {
        let buff = self.current_buff.take().unwrap();

        let current_len = buff.filled().len();
        self.request_length += current_len;
        if self.request_length < self.limit_length {
            permit.send(Ok(buff));
        } else {
            Self::send_error(std::io::Error::other("request length"), permit);
        }
    }

    fn send_delimited(
        &mut self,
        delimited: &Range<usize>,
        permit: Permit<'_, Result<BufferChunk, std::io::Error>>,
    ) {
        let mut next_buff = self.next_buff.take().expect("next_buff");
        let mut buff = self.current_buff.take().expect("current_buff");
        buff.carry_over(&mut next_buff, delimited, self.delimiter);
        self.current_buff = Some(buff);

        self.send_buffer(permit);
        self.request_length = 0;
        self.current_buff = Some(next_buff);
    }

    fn send_error(err: std::io::Error, permit: Permit<'_, Result<BufferChunk, std::io::Error>>) {
        permit.send(Err(err));
    }

    async fn fill(&mut self) {
        let buff = self.current_buff.as_mut().unwrap();
        let fill_result = buff
            .fill_from(&mut self.source, self.delimiter, self.idle_timeout)
            .await;
        self.fill_result = Some(fill_result);
    }

    fn current_is_full(&self) -> bool {
        if let Some(buff) = &self.current_buff {
            buff.is_full()
        } else {
            false
        }
    }

    fn current_delimited(&self) -> Option<Range<usize>> {
        if let Some(buff) = &self.current_buff {
            buff.delimited()
        } else {
            None
        }
    }

    /// reading pipe
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when IO error
    ///
    /// # Panics
    ///
    /// when logic Bug
    ///
    pub async fn poll_source(&mut self) -> Result<bool, std::io::Error> {
        self.reserve_next_buff().await?;
        if self.current_buff.is_none() {
            self.current_buff = self.next_buff.take();
            self.reserve_next_buff().await?;
        }

        let buffer_tx = self.buffer_tx.clone();
        let permit = buffer_tx
            .reserve()
            .await
            .map_err(|_| std::io::Error::other("No receiver"))?;

        if self.fill_result.is_none() {
            if let Some(delimited) = self.current_delimited() {
                self.send_delimited(&delimited, permit);
            } else {
                self.fill().await;
            }
        } else {
            let fill_result = self.fill_result.take().expect("fill_result");

            match fill_result {
                Err(err) => {
                    Self::send_error(err, permit);
                }
                Ok((filled, eof)) => {
                    if eof {
                        self.send_buffer(permit);
                        self.eof = true
                    } else if let Some(delimited) = filled {
                        self.send_delimited(&delimited, permit);
                    } else if self.current_is_full() {
                        self.send_buffer(permit);
                    }
                }
            }
        }
        Ok(self.eof)
    }

    async fn send_canceled(&mut self) -> Result<(), std::io::Error> {
        let permit = self
            .buffer_tx
            .reserve()
            .await
            .map_err(|_| std::io::Error::other("No receiver"))?;
        permit.send(Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "operation canceled",
        )));
        Ok(())
    }

    const fn set_request_length_limit(&mut self, new_limit: usize) {
        self.limit_length = new_limit;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("buffer recycle error: SEND")]
    Send(#[from] tokio::sync::mpsc::error::SendError<BufferChunk>),

    #[error(transparent)]
    Deser(#[from] serde_json::Error),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct JsonLinesStreamReader<AR>
where
    AR: tokio::io::AsyncRead + tokio::io::AsyncReadExt + std::marker::Unpin + Send + Sync,
{
    readable_buffer: Arc<std::sync::Mutex<ReadableBuffer>>,
    pipe_buf: Arc<Mutex<PipeBuffer<AR>>>,
}

impl<AR> JsonLinesStreamReader<AR>
where
    AR: tokio::io::AsyncRead + tokio::io::AsyncReadExt + std::marker::Unpin + Send + Sync + 'static,
{
    pub async fn new_async(
        read_half: AR,
        buffer_count: usize,
        buffer_size: usize,
        delimiter: u8,
        request_length_limit: usize,
        idle_timeout: Duration,
    ) -> Result<Self, ReaderError> {
        if buffer_count < 2 {
            return Err(std::io::Error::other("require 2 buffers"))?;
        }
        let (buf_tx, buf_rx) = tokio::sync::mpsc::channel(buffer_count);
        let (ret_tx, ret_rx) = tokio::sync::mpsc::channel(buffer_count);

        for _ in 0..buffer_count {
            ret_tx.send(BufferChunk::with_capacity(buffer_size)).await?;
        }

        let pipe_buf = PipeBuffer {
            buffer_tx: buf_tx,
            return_rx: ret_rx,
            current_buff: None,
            delimiter,
            source: read_half,
            limit_length: request_length_limit,
            request_length: 0,
            idle_timeout,
            fill_result: None,
            next_buff: None,
            eof: false,
        };
        let readable_buffer = ReadableBuffer {
            buffer_rx: buf_rx,
            return_tx: ret_tx,
            current_buffer: None,
        };

        Ok(Self {
            readable_buffer: Arc::new(std::sync::Mutex::new(readable_buffer)),
            pipe_buf: Arc::new(Mutex::new(pipe_buf)),
        })
    }

    pub async fn set_request_length_limit(&mut self, new_limit: usize) {
        let mut pipe_buf = self.pipe_buf.lock().await;
        pipe_buf.set_request_length_limit(new_limit);
    }

    pub async fn read_request(&mut self) -> Result<serde_json::Value, ReaderError> {
        let pipe_buf = self.pipe_buf.clone();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let poller = tokio::spawn(async move {
            let mut pipe_buf = pipe_buf.lock().await;
            loop {
                tokio::select! {
                    result = pipe_buf.poll_source() => {
                        match result {
                            Ok(true) => return Ok(true),
                            Ok(false) => {},
                            Err(e) => {
                                return Err(e)
                            }
                        }
                    }
                    Ok(()) = &mut cancel_rx => {
                        pipe_buf.send_canceled().await?;
                        return Ok(false)
                    }
                    _ = &mut stop_rx => return Ok(false),
                }
            }
        });
        let readable_buffer = self.readable_buffer.clone();

        let join_handle =
            tokio::task::spawn_blocking(move || -> Result<serde_json::Value, ReaderError> {
                use serde::Deserialize;

                let mut readable_buffer = readable_buffer.lock().unwrap();
                let mut deserializer = serde_json::Deserializer::from_reader(&mut *readable_buffer);
                let value = serde_json::Value::deserialize(&mut deserializer)?;
                let mut buf = [0u8; 1];
                loop {
                    readable_buffer.read_exact(&mut buf)?;
                    if buf[0] == b' ' || buf[0] == b'\t' || buf[0] == b'\r' {
                        continue;
                    }
                    break;
                }
                if buf[0] == b'\n' {
                    Ok(value)
                } else {
                    Err(
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, "require new line")
                            .into(),
                    )
                }
            });

        let cancel_guard = scopeguard::guard(cancel_tx, |tx| {
            let _ = tx.send(());
        });

        let value = join_handle.await;
        let cancel_tx = scopeguard::ScopeGuard::into_inner(cancel_guard);
        drop(cancel_tx);

        let _ = stop_tx.send(());

        let poller_result = poller.await?;
        poller_result?;

        value?
    }
}

#[cfg(test)]
mod read_request_test {
    use std::{io::Cursor, time::Duration};
    use testresult::TestResult;

    use crate::buffers::JsonLinesStreamReader;

    #[tokio::test(flavor = "multi_thread")]
    #[rstest::rstest]
    #[case(1)]
    #[case(2)]
    #[case(3)]
    #[case(4)]
    #[case(5)]
    #[case(6)]
    async fn test_read_request(#[case] count: usize) -> TestResult {
        let input_item = r#"{"field":"value"}"#;
        let inputs = [input_item];
        let inputs = inputs.repeat(count);
        let mut input = inputs.join("\n");
        input.push('\n');

        let cursor = Cursor::new(input);
        let mut reader =
            JsonLinesStreamReader::new_async(cursor, 3, 8192, b'\n', 1024, Duration::from_secs(30))
                .await?;

        for _n in 0..count {
            let value = reader.read_request().await?;
            let obj = value.as_object().unwrap();

            assert_eq!(obj.get("field"), Some(&serde_json::json! {"value"}));
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_big_json_input() -> TestResult {
        let input_json = include_str!("../../sql-fun-home/postgres/17/pg_type.json");
        let mut input_json = input_json.replace('\n', " ");
        input_json.push('\n');
        let cursor = Cursor::new(input_json);
        let mut reader = JsonLinesStreamReader::new_async(
            cursor,
            4,
            512,
            b'\n',
            100_000_000,
            Duration::from_secs(30),
        )
        .await?;

        reader.read_request().await?;
        Ok(())
    }
}
