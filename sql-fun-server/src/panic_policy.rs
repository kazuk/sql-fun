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

// Removed the subprocess panic/backtrace test; restore from df446be6c1d950353fbd7f79916f79c68bfd396d.
