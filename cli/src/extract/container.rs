use crate::extract::bundle::EXPORT_SCRIPT;

use std::{process::Stdio, time::Duration};

use ::anyhow::Context;
use ::tokio::{io::AsyncWriteExt, process::Command};

pub(super) struct Container {
    id: String,
    password: String,
}

impl Container {
    pub(super) async fn create(image: &str, bind: &str) -> Result<Self, anyhow::Error> {
        anyhow::ensure!(!image.starts_with('-'), "invalid Docker image reference");
        let _: std::net::IpAddr = bind
            .parse()
            .context("Docker bind address must be an IP address")?;
        // No container or volume exists during image retrieval.
        if command(&["image", "inspect", image], None).await.is_err() {
            command(&["pull", image], None).await?;
        }
        let secret = tempfile::Builder::new()
            .prefix("sql-fun-secret-")
            .rand_bytes(32)
            .tempdir()?;
        let password = secret
            .path()
            .file_name()
            .context("temporary secret")?
            .to_string_lossy()
            .into_owned();
        let env = secret.path().join("postgres.env");
        std::fs::write(&env, format!("POSTGRES_PASSWORD={password}\n"))?;
        let cid = secret.path().join("container.id");
        let identity = tempfile::Builder::new()
            .prefix("sql-fun-extract-")
            .rand_bytes(16)
            .tempdir()?;
        let name = identity
            .path()
            .file_name()
            .context("temporary container name")?
            .to_string_lossy()
            .into_owned();
        let published = if bind.contains(':') {
            format!("[{bind}]::5432")
        } else {
            format!("{bind}::5432")
        };
        let result = command(
            &[
                "create",
                "--name",
                &name,
                "--label",
                "io.sql-fun.extraction=owned",
                "--cidfile",
                cid.to_str().context("container ID path")?,
                "--env-file",
                env.to_str().context("environment path")?,
                "--publish",
                &published,
                image,
            ],
            None,
        )
        .await;
        if let Err(error) = result {
            if let Ok(id) = std::fs::read_to_string(&cid) {
                command(&["rm", "--force", "--volumes", id.trim()], None).await?;
            }
            return Err(error.context(format!("Docker creation failed; if the daemon completed after timeout, inspect task resource {name}")));
        }
        let id = std::fs::read_to_string(cid)?.trim().to_owned();
        Ok(Self { id, password })
    }

    pub(super) async fn start(&self) -> Result<(), anyhow::Error> {
        command(&["start", &self.id], None).await?;
        Ok(())
    }

    pub(super) async fn connection(&self, host: &str) -> Result<String, anyhow::Error> {
        let bytes = command(
            &[
                "inspect",
                "--format",
                "{{json .NetworkSettings.Ports}}",
                &self.id,
            ],
            None,
        )
        .await?;
        let ports: serde_json::Value = serde_json::from_slice(&bytes)?;
        let port: u16 = ports["5432/tcp"][0]["HostPort"]
            .as_str()
            .context("Docker did not publish PostgreSQL port")?
            .parse()?;
        let mut url = url::Url::parse("postgresql://postgres@localhost/postgres")?;
        url.set_host(Some(host))
            .map_err(|_| anyhow::anyhow!("invalid Docker host address"))?;
        url.set_port(Some(port))
            .map_err(|()| anyhow::anyhow!("invalid PostgreSQL port"))?;
        url.set_password(Some(&self.password))
            .map_err(|()| anyhow::anyhow!("invalid generated credentials"))?;
        url.set_query(Some("sslmode=disable"));
        Ok(url.to_string())
    }

    pub(super) async fn provenance(&self) -> Result<serde_json::Value, anyhow::Error> {
        let image = command(&["inspect", "--format", "{{.Image}}", &self.id], None).await?;
        Ok(serde_json::json!({"source": "docker", "image_id": String::from_utf8(image)?.trim()}))
    }

    pub(super) async fn export(&self, output: &std::path::Path) -> Result<(), anyhow::Error> {
        let file = std::fs::File::create(output)?;
        let mut child = Command::new("docker")
            .args(["exec", "-i", &self.id, "sh", "-s"])
            .stdin(Stdio::piped())
            .stdout(file)
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("Docker CLI is required for extension export")?;
        child
            .stdin
            .take()
            .context("Docker stdin")?
            .write_all(EXPORT_SCRIPT.as_bytes())
            .await?;
        let status = child.wait().await?;
        anyhow::ensure!(
            status.success(),
            "Docker extension export failed (details redacted)"
        );
        Ok(())
    }

    pub(super) async fn remove(&mut self) -> Result<(), anyhow::Error> {
        command(&["rm", "--force", "--volumes", &self.id], None).await
            .with_context(|| format!("remove only task-owned container {} manually if the Docker daemon remains unavailable", self.id))?;
        Ok(())
    }
}

async fn command(arguments: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, anyhow::Error> {
    let operation = async {
        let mut child = Command::new("docker")
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Docker CLI is required for the Docker source")?;
        if let Some(input) = input {
            child
                .stdin
                .take()
                .context("Docker stdin")?
                .write_all(input)
                .await?;
        } else {
            drop(child.stdin.take());
        }
        let output = child.wait_with_output().await?;
        anyhow::ensure!(
            output.status.success(),
            "Docker {} failed (daemon output redacted)",
            arguments.first().unwrap_or(&"command")
        );
        Ok(output.stdout)
    };
    tokio::time::timeout(Duration::from_secs(120), operation)
        .await
        .context("Docker operation timed out")?
}
