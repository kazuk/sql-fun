use std::{
    path::PathBuf,
    process::{ExitCode, Stdio},
    time::Duration,
};

use crate::CliSubCommand;
use anyhow::Context;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::{Child, Command},
};

use crate::SqlFunCliArgs;

/// command output / error stream
pub enum StdStream {
    /// stdout line
    Stdout(String),
    /// stderr line
    Stderr(String),
}

/// runnable command wrapper for [`CliSubCommand`]
pub struct RunnableCommand {
    binary: PathBuf,
    sub_command: CliSubCommand,
}

impl TryFrom<&CliSubCommand> for RunnableCommand {
    type Error = anyhow::Error;

    fn try_from(value: &CliSubCommand) -> Result<Self, Self::Error> {
        let binary_names = value.binary_names();
        let mut binary_paths = Vec::new();

        for bin in &binary_names {
            if let Ok(bin_path) = which::which(bin) {
                binary_paths.push(bin_path);
            }
        }

        if binary_paths.is_empty() {
            anyhow::bail!("expected binary not found {}", binary_names.join(", "))
        }
        if binary_paths.len() > 1 {
            anyhow::bail!("ambiguous binary found {binary_paths:?}")
        }

        Ok(Self {
            binary: binary_paths[0].clone(),
            sub_command: value.clone(),
        })
    }
}

impl RunnableCommand {
    #[cfg(unix)]
    fn send_sigterm(child: &Child) -> Result<(), anyhow::Error> {
        use ::nix::unistd::Pid;
        use nix::sys::signal::Signal;
        let pid = Pid::from_raw(Self::pid_as_i32(child)?);
        nix::sys::signal::kill(pid, Signal::SIGTERM)?;
        Ok(())
    }

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

    /// execute binary
    ///
    /// # Errors
    ///
    /// Returns an error when spawning or waiting on the child process fails.
    pub async fn execute(&self, args: &SqlFunCliArgs) -> Result<ExitCode, anyhow::Error> {
        self.sub_command.execute_external(self, args).await
    }

    pub(crate) async fn run_external(
        &self,
        core_args: &SqlFunCliArgs,
        args: &[String],
    ) -> Result<i32, anyhow::Error> {
        let envs = core_args.common_args.environments()?;
        let mut child = Command::new(&self.binary)
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
        let mut stdout = BufReader::new(stdout);
        let mut stderr = BufReader::new(stderr);
        let mut stdout_line_buf = String::new();
        let mut stderr_line_buf = String::new();
        let mut stream_items = Vec::new();
        let mut stderr_eof = false;
        let mut stdout_eof = false;
        #[cfg(unix)]
        let mut term_sent = false;
        let timeout = Duration::from_secs(30);
        let grace_timer = tokio::time::sleep(Duration::MAX);
        tokio::pin!(grace_timer);
        let status_result = loop {
            let stream_item = tokio::select! {
                biased;
                line = Self::read_line_from( &mut stderr,&mut stderr_line_buf, &mut stderr_eof ), if !stderr_eof
                => {
                    let Some(line) = line? else {
                        continue;
                    };
                    eprint!("{line}");
                    StdStream::Stderr(line)
                },
                line = Self::read_line_from( &mut stdout,&mut stdout_line_buf, &mut stdout_eof ), if !stdout_eof
                => {
                    let Some(line) = line? else { continue; };
                    print!("{line}");
                    StdStream::Stdout(line)
                },
                status = child.wait() => {
                    break status;
                },
                () = tokio::time::sleep(timeout) => {
                    #[cfg(unix)]
                    if !term_sent {
                        let pid = Self::pid_as_i32(&child)?;
                        eprintln!("Timeout reached, sending SIGTERM {pid}");
                        Self::send_sigterm(&child)?;
                        grace_timer.as_mut().reset(tokio::time::Instant::now() + timeout);
                        term_sent =true;
                        continue;
                    }
                    eprintln!("Timeout reached, killing process");
                    child.start_kill()?;
                    continue;
                },
                () = &mut grace_timer => {
                    eprintln!("Terminating");
                    child.start_kill()?;
                    continue;
                }
            };
            stream_items.push(stream_item);
        };
        if !stderr_eof {
            while stderr.read_line(&mut stderr_line_buf).await? != 0 {
                eprint!("{stderr_line_buf}");
                stream_items.push(StdStream::Stderr(stderr_line_buf.clone()));
                stderr_line_buf.clear();
            }
        }
        if !stdout_eof {
            while stdout.read_line(&mut stdout_line_buf).await? != 0 {
                print!("{stdout_line_buf}");
                stream_items.push(StdStream::Stdout(stdout_line_buf.clone()));
                stdout_line_buf.clear();
            }
        }
        let status_result = status_result?;
        let exit_code = status_result.code().unwrap_or_else(|| {
            #[cfg(windows)]
            {
                unreachable!()
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                128 + status_result.signal().unwrap_or(0)
            }
        });
        Ok(exit_code)
    }

    #[cfg(unix)]
    fn pid_as_i32(child: &Child) -> Result<i32, anyhow::Error> {
        let pid = child.id().context("child has pid")?;
        i32::try_from(pid).context("pid does not fit into i32")
    }
}
