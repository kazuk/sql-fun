use ::anyhow::Context;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// A selected executable, never a shell command or a fallback after failure.
pub(super) struct Downloader {
    executable: PathBuf,
    curl: bool,
}
impl Downloader {
    pub(super) fn find() -> Result<Self, anyhow::Error> {
        if let Ok(executable) = which::which("curl") {
            return Ok(Self {
                executable,
                curl: true,
            });
        }
        let executable = which::which("wget").context(
            "theme download requires curl or wget with TLS support; install one and rerun init",
        )?;
        Ok(Self {
            executable,
            curl: false,
        })
    }
    fn command(&self, url: &str, destination: &Path) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.executable);
        if self.curl {
            command.args([
                "--disable",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-redirs",
                "5",
                "--connect-timeout",
                "10",
                "--max-time",
                "30",
                "--write-out",
                "%{http_code}",
                "--output",
            ]);
        } else {
            // No redirects: wget's --https-only does not constrain ordinary redirects.
            command.args([
                "--no-config",
                "--no-hsts",
                "--no-netrc",
                "--server-response",
                "--https-only",
                "--max-redirect=0",
                "--tries=1",
                "--timeout=10",
                "--output-document",
            ]);
        }
        command
            .arg(destination)
            .arg(url)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        command
    }
    fn http_status(&self, output: &std::process::Output) -> Result<u16, anyhow::Error> {
        if self.curl {
            return Ok(std::str::from_utf8(&output.stdout)?.trim().parse()?);
        }
        // GNU wget reports response status lines on stderr with --server-response.
        let response = std::str::from_utf8(&output.stderr)?;
        let status = response
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                if line.starts_with("HTTP/") {
                    line.split_whitespace().nth(1)
                } else {
                    None
                }
            })
            .next_back()
            .context("wget did not report an HTTP response status")?;
        Ok(status.parse()?)
    }
    pub(super) async fn download(
        &self,
        url: &str,
        destination: &Path,
    ) -> Result<(), anyhow::Error> {
        let mut command = self.command(url, destination);
        let output = tokio::time::timeout(Duration::from_secs(35), command.output())
            .await
            .context("theme download timed out")?
            .with_context(|| format!("could not start {}", self.executable.display()))?;
        anyhow::ensure!(
            output.status.success(),
            "theme download from {url} failed: {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let status = self.http_status(&output)?;
        anyhow::ensure!(
            (200..300).contains(&status),
            "theme download from {url} returned HTTP {status}"
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_download {
    use super::Downloader;
    use crate::test_helpers::theme::ThemeFixture;

    #[rstest::rstest]
    #[case::curl(true)]
    #[case::wget(false)]
    #[tokio::test]
    async fn checks_downloader_exit_status(#[case] curl: bool) {
        let fixture = ThemeFixture::new();
        let executable = fixture.program("downloader", "exit 22");
        let downloader = Downloader { executable, curl };
        assert!(
            downloader
                .download("https://example.invalid/theme.toml", &fixture.theme())
                .await
                .is_err()
        );
        assert!(!fixture.theme().exists());
    }
    #[rstest::rstest]
    #[case::curl_redirect(true, "printf 302")]
    #[case::wget_not_modified(false, "printf '  HTTP/1.1 304 Not Modified\\n' >&2")]
    #[tokio::test]
    async fn successful_process_exit_requires_successful_http_status(
        #[case] curl: bool,
        #[case] body: &str,
    ) {
        let fixture = ThemeFixture::new();
        let executable = fixture.program("downloader", body);
        let downloader = Downloader { executable, curl };
        assert!(
            downloader
                .download("https://example.invalid/theme.toml", &fixture.theme())
                .await
                .is_err()
        );
    }
}
