use std::{
    collections::HashMap,
    process::{ExitStatus, Stdio},
    time::Duration,
};

use anyhow::Context;

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
    #[cfg_attr(not(test), expect(dead_code))]
    pub fn start_new(
        command: &str,
        args: &[&str],
        envs: &HashMap<String, String>,
    ) -> Result<Self, anyhow::Error> {
        let mut child = Command::new(command)
            .env_clear()
            .envs(envs)
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

        Ok(Self {
            child,
            stdout,
            stderr,
            stream_items: Vec::default(),
        })
    }
}

impl ChildProcess {
    #[cfg_attr(not(test), expect(dead_code))]
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
}

#[cfg(test)]
mod test_child_process_read_line_from {
    use super::ChildProcess;
    use crate::test_helpers::{make_duplex_stream, write_lines};
    use tokio::io::BufReader;

    #[tokio::test]
    async fn reads_lines_and_tracks_eof() {
        let (mut writer, reader) = make_duplex_stream(64);
        let write_task =
            tokio::spawn(async move { write_lines(&mut writer, &["one\n", "two\n"]).await });

        let mut reader = BufReader::new(reader);
        let mut buffer = String::new();
        let mut eof = false;

        let first = ChildProcess::read_line_from(&mut reader, &mut buffer, &mut eof)
            .await
            .expect("read first line")
            .expect("first line");
        assert_eq!("one\n", first);
        assert!(buffer.is_empty());
        assert!(!eof);

        let second = ChildProcess::read_line_from(&mut reader, &mut buffer, &mut eof)
            .await
            .expect("read second line")
            .expect("second line");
        assert_eq!("two\n", second);
        assert!(buffer.is_empty());
        assert!(!eof);

        let none = ChildProcess::read_line_from(&mut reader, &mut buffer, &mut eof)
            .await
            .expect("read eof");
        let write_result = write_task.await.expect("writer task");

        assert!(none.is_none());
        assert!(eof);
        assert!(write_result.is_ok());
    }
}

impl ChildProcess {
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
}

#[cfg(test)]
mod test_child_process_read_from_buffer {
    use super::{ChildProcess, LinesBufReader};
    use crate::test_helpers::{make_duplex_stream, write_lines};

    #[tokio::test]
    async fn reads_lines_until_eof() {
        let (mut writer, reader) = make_duplex_stream(64);
        let write_task =
            tokio::spawn(async move { write_lines(&mut writer, &["first\n", "second\n"]).await });

        let mut source = LinesBufReader::new(reader);
        let first = ChildProcess::read_from_buffer(&mut source)
            .await
            .expect("read first line")
            .expect("first line");
        let second = ChildProcess::read_from_buffer(&mut source)
            .await
            .expect("read second line")
            .expect("second line");
        let none = ChildProcess::read_from_buffer(&mut source)
            .await
            .expect("read eof");

        let write_result = write_task.await.expect("writer task");

        assert_eq!("first\n", first);
        assert_eq!("second\n", second);
        assert!(none.is_none());
        assert!(source.eof);
        assert!(write_result.is_ok());
    }
}

impl ChildProcess {
    async fn read_to_eof_from<Stdio>(
        source: &mut LinesBufReader<Stdio>,
    ) -> Result<Vec<String>, anyhow::Error>
    where
        Stdio: AsyncRead + Unpin,
    {
        let mut results = Vec::new();
        while source.reader.read_line(&mut source.line_buffer).await? != 0 {
            results.push(source.line_buffer.clone());
            source.line_buffer.clear();
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test_child_process_read_to_eof_from {
    use super::{ChildProcess, LinesBufReader};
    use crate::test_helpers::{make_duplex_stream, write_lines};

    #[tokio::test]
    async fn collects_remaining_lines() {
        let (mut writer, reader) = make_duplex_stream(64);
        let write_task = tokio::spawn(async move {
            write_lines(&mut writer, &["alpha\n", "beta\n", "gamma\n"]).await
        });

        let mut source = LinesBufReader::new(reader);
        let lines = ChildProcess::read_to_eof_from(&mut source)
            .await
            .expect("read to eof");

        let write_result = write_task.await.expect("writer task");

        assert_eq!(vec!["alpha\n", "beta\n", "gamma\n"], lines);
        assert!(source.line_buffer.is_empty());
        assert!(write_result.is_ok());
    }
}

impl ChildProcess {
    #[cfg_attr(not(test), expect(dead_code))]
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
                    eprint!("{line}");
                    StdStream::Stderr(line)
                },
                line = Self::read_from_buffer(&mut self.stdout), if !self.stdout.eof
                => {
                    let Some(line) = line? else { continue; };
                    print!("{line}");
                    StdStream::Stdout(line)
                },
                status = self.child.wait() => {
                    break status;
                },
                () = sleep(timeout) => {
                    #[cfg(unix)]
                    if !term_sent {
                        let pid = Self::pid_as_i32(&self.child)?;
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
                () = &mut grace_timer => {
                    eprintln!("Terminating");
                    self.child.start_kill()?;
                    continue;
                }
            };
            self.stream_items.push(stream_item);
        };
        self.read_to_eof().await?;
        Ok(status_result?)
    }
}

#[cfg(test)]
#[cfg(unix)]
mod test_child_process_poll_exit {
    use super::ChildProcess;
    use crate::StdStream;
    use std::collections::HashMap;

    #[tokio::test]
    async fn collects_stdout_and_stderr() {
        let envs = HashMap::new();
        let mut child = ChildProcess::start_new(
            "/bin/sh",
            &["-c", "printf 'out1\\nout2\\n'; printf 'err1\\n' 1>&2"],
            &envs,
        )
        .expect("start child");

        let status = child.poll_exit().await.expect("poll exit");

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        for item in &child.stream_items {
            match item {
                StdStream::Stdout(line) => stdout_lines.push(line.clone()),
                StdStream::Stderr(line) => stderr_lines.push(line.clone()),
            }
        }

        assert!(status.success());
        assert_eq!(vec!["out1\n", "out2\n"], stdout_lines);
        assert_eq!(vec!["err1\n"], stderr_lines);
    }
}

impl ChildProcess {
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
}

impl ChildProcess {
    #[cfg(unix)]
    fn send_sigterm(child: &Child) -> Result<(), anyhow::Error> {
        use ::nix::unistd::Pid;
        use nix::sys::signal::Signal;
        let pid = Pid::from_raw(Self::pid_as_i32(child)?);
        nix::sys::signal::kill(pid, Signal::SIGTERM)?;
        Ok(())
    }
}

impl ChildProcess {
    #[cfg(unix)]
    fn pid_as_i32(child: &Child) -> Result<i32, anyhow::Error> {
        let pid = child.id().context("child has pid")?;
        i32::try_from(pid).context("pid does not fit into i32")
    }
}
