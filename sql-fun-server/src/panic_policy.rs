/// Install the temporary server-only fail-fast policy before starting its runtime.
///
/// The previous hook prints diagnostics/backtraces before the process aborts.
/// Importing the server library does not install this process-wide hook.
pub fn install_server_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |information| {
        previous(information);
        std::process::abort();
    }));
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn aborts_spawned_task_after_printing_backtrace() {
        const CHILD: &str = "SQL_FUN_PANIC_POLICY_CHILD";
        if std::env::var_os(CHILD).is_some() {
            super::install_server_panic_hook();
            let runtime = ::tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                ::tokio::spawn(async { panic!("schema loader panic probe") })
                    .await
                    .unwrap();
            });
            panic!("server continued after panic");
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "panic_policy::tests::aborts_spawned_task_after_printing_backtrace",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("RUST_BACKTRACE", "1")
            .output()
            .unwrap();
        assert_eq!(output.status.signal(), Some(6));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("schema loader panic probe"), "{stderr}");
        assert!(stderr.contains("stack backtrace"), "{stderr}");
    }
}
