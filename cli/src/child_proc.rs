use std::{
    collections::HashMap,
    process::{ExitStatus, Stdio},
    time::Duration,
};

#[cfg(unix)]
use tokio::process::Child;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
};

use crate::StdStream;

struct LinesBufReader<Read>
where
    Read: AsyncRead + Unpin,
{
    reader: BufReader<Read>,
    line_buffer: String,
    eof: bool,
}

impl<Read> LinesBufReader<Read>
where
    Read: AsyncRead + Unpin,
{
    pub fn new(read: Read) -> Self {
        Self {
            reader: BufReader::new(read),
            line_buffer: String::new(),
            eof: false,
        }
    }
}

struct ChildProcess {
    child: Child,
    stdout: LinesBufReader<tokio::process::ChildStdout>,
    stderr: LinesBufReader<tokio::process::ChildStderr>,
    stream_items: Vec<StdStream>,
}

impl ChildProcess {
    #[expect(dead_code)]
    pub async fn start_new(
        command: &str,
        args: &[&str],
        envs: HashMap<String, String>,
    ) -> Result<Self, anyhow::Error> {
        let mut child = Command::new(command)
            .env_clear()
            .envs(&envs)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let Some(stdout) = child.stdout.take() else {
            anyhow::bail!("can not to get STDOUT")
        };
        let Some(stderr) = child.stderr.take() else {
            anyhow::bail!("can not to get STDERR")
        };
        let stdout = LinesBufReader::new(stdout);
        let stderr = LinesBufReader::new(stderr);

        Ok(ChildProcess {
            child,
            stdout,
            stderr,
            stream_items: Default::default(),
        })
    }

    #[expect(dead_code)]
    async fn read_line_from<Stdio>(
        from: &mut BufReader<Stdio>,
        buffer: &mut String,
        eof: &mut bool,
    ) -> Result<Option<String>, anyhow::Error>
    where
        Stdio: AsyncRead + Unpin,
    {
        let len = from.read_line(buffer).await?;
        if len == 0 {
            *eof = true;
            Ok(None)
        } else {
            let result = buffer.clone();
            buffer.clear();
            Ok(Some(result))
        }
    }

    async fn read_from_buffer<Stdio>(
        source: &mut LinesBufReader<Stdio>,
    ) -> Result<Option<String>, anyhow::Error>
    where
        Stdio: AsyncRead + Unpin,
    {
        let len = source.reader.read_line(&mut source.line_buffer).await?;
        if len == 0 {
            source.eof = true;
            Ok(None)
        } else {
            let result = source.line_buffer.clone();
            source.line_buffer.clear();
            Ok(Some(result))
        }
    }

    async fn read_to_eof_from<Stdio>(
        source: &mut LinesBufReader<Stdio>,
    ) -> Result<Vec<String>, anyhow::Error>
    where
        Stdio: AsyncRead + Unpin,
    {
        let mut results = Vec::new();
        while source.reader.read_line(&mut source.line_buffer).await? != 0 {
            results.push(source.line_buffer.to_string());
            source.line_buffer.clear();
        }
        Ok(results)
    }

    #[expect(dead_code)]
    pub async fn poll_exit(&mut self) -> Result<ExitStatus, anyhow::Error> {
        use tokio::time::sleep;

        #[cfg(unix)]
        let mut term_sent = false;
        let timeout = Duration::from_secs(30);
        let grace_timer = sleep(Duration::MAX);
        tokio::pin!(grace_timer);
        let status_result = loop {
            let stream_item = tokio::select! {
                biased;
                line = Self::read_from_buffer(&mut self.stderr), if !self.stderr.eof
                => {
                    let Some(line) = line? else {
                        continue;
                    };
                    eprint!("{}", &line);
                    StdStream::Stderr(line)
                },
                line = Self::read_from_buffer(&mut self.stdout), if !self.stdout.eof
                => {
                    let Some(line) = line? else { continue; };
                    print!("{}",&line);
                    StdStream::Stdout(line)
                },
                status = self.child.wait() => {
                    break status;
                },
                _ = sleep(timeout) => {
                    #[cfg(unix)]
                    if !term_sent {
                        let pid = self.child.id().expect("child has pid") as i32;
                        eprintln!("Timeout reached, sending SIGTERM {pid}");
                        Self::send_sigterm(&self.child)?;
                        grace_timer.as_mut().reset(tokio::time::Instant::now() + timeout);
                        term_sent =true;
                        continue;
                    }
                    eprintln!("Timeout reached, killing process");
                    self.child.start_kill()?;
                    continue;
                },
                _ = &mut grace_timer => {
                    eprintln!("Terminating");
                    self.child.start_kill()?;
                    continue;
                }
            };
            self.stream_items.push(stream_item)
        };
        self.read_to_eof().await?;
        Ok(status_result?)
    }

    async fn read_to_eof(&mut self) -> Result<(), anyhow::Error> {
        if !self.stderr.eof {
            let lines = Self::read_to_eof_from(&mut self.stderr).await?;
            self.stream_items
                .append(&mut lines.into_iter().map(StdStream::Stderr).collect());
        }
        if !self.stdout.eof {
            let lines = Self::read_to_eof_from(&mut self.stdout).await?;
            self.stream_items
                .append(&mut lines.into_iter().map(StdStream::Stdout).collect());
        }
        Ok(())
    }

    #[cfg(unix)]
    fn send_sigterm(child: &Child) -> Result<(), anyhow::Error> {
        use ::nix::unistd::Pid;
        use nix::sys::signal::Signal;
        let pid = child.id().expect("child has pid") as i32;
        nix::sys::signal::kill(Pid::from_raw(pid), Signal::SIGTERM)?;
        Ok(())
    }
}
