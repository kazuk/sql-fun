use std::path::{Path, PathBuf};

#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::OpenOptionsExt;

use crate::{Args, ServerError};

#[derive(serde::Deserialize, serde::Serialize, derive_getters::Getters)]
pub struct PidFile {
    pid: u32,
    port: u16,
    token: String,

    #[serde(skip)]
    pid_file_path: Option<PathBuf>,
    #[serde(skip)]
    pid_file: Option<std::fs::File>,
}

impl Drop for PidFile {
    fn drop(&mut self) {
        if let Some(pid_file_path) = &self.pid_file_path {
            let _ = std::fs::remove_file(pid_file_path); // ignore error
        }
    }
}

impl PidFile {
    fn new(port: u16) -> Self {
        let mut arr = [0u8; 20];
        ::rand::fill(&mut arr[..]);
        let token = arr.iter().map(|b| format!("{:02x}", b)).collect();
        Self {
            pid: std::process::id(),
            pid_file_path: None,
            pid_file: None,
            token,
            port,
        }
    }

    fn default_path(args: &Args) -> PathBuf {
        let mut pid_path = PathBuf::from("/tmp");
        pid_path.push(&args.user);
        pid_path.push("sql-fun-server.pid");
        pid_path
    }

    fn from_file(path: &Path) -> Result<Self, ServerError> {
        let pid_file_content = std::fs::read_to_string(&path)?;
        let pid_file: Self = serde_json::from_str(&pid_file_content)?;
        Ok(pid_file)
    }

    fn process_exists(&self) -> bool {
        let mut sys = sysinfo::System::new();
        sys.refresh_specifics(
            sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::default()),
        );
        sys.process(sysinfo::Pid::from_u32(self.pid)).is_some()
    }

    fn write_to_path(&mut self, path: &Path) -> Result<(), ServerError> {
        use std::fs::OpenOptions;
        use std::io::Write;

        if self.pid_file.is_some() || self.pid_file_path.is_some() {
            return Err(ServerError::Bug(String::from("pid file ready persisted")));
        }
        // make sure create parent dir
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string(self)?;

        #[cfg(not(target_os = "windows"))]
        let mut file = OpenOptions::new()
            .mode(0o600)
            .write(true)
            .create_new(true)
            .open(path)?;

        #[cfg(target_os = "windows")]
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;

        file.write_all(content.as_bytes())?;
        fs2::FileExt::try_lock_shared(&file)?;

        self.pid_file = Some(file);
        self.pid_file_path = Some(path.to_path_buf());
        Ok(())
    }

    /// Attempts to register this process as the primary server.
    ///
    /// # Returns
    ///
    /// - `None`: Another process is already running as the primary server. This process should exit.
    /// - `Some`: This process has successfully registered as the primary server.
    ///
    pub fn try_register_primary_server(
        args: &Args,
        port: u16,
    ) -> Result<Option<Self>, ServerError> {
        let pid_path = PidFile::default_path(&args);
        if pid_path.exists() {
            let pid_file = PidFile::from_file(&pid_path)?;
            if pid_file.process_exists() {
                return Ok(None);
            }
            std::fs::remove_file(&pid_path)?;
        }
        let mut pid_file = PidFile::new(port);
        pid_file.write_to_path(&pid_path)?;
        Ok(Some(pid_file))
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::{Args, pid_file::PidFile};

    #[test]
    pub fn test_try_register_primary_server() -> testresult::TestResult {
        let port = 30000;
        let args = Args::try_parse_from(&vec![""])?;
        PidFile::try_register_primary_server(&args, port)?;

        Ok(())
    }
}
