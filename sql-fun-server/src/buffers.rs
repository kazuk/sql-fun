use tokio::io::AsyncBufReadExt;

/// JSON RPC TCP Stream buffer
pub struct ReadableBuffer {
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

/// Pipe buffer
pub struct PipeBuffer {
    buffer_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    return_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

impl PipeBuffer {
    /// reading pipe
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when IO error
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
