use std::path::Path;

/// Explicit boundary for future `SQLite` development-support setup tasks.
pub(super) fn initialize(_home: &Path) {
    println!("SQLite development-support setup is still under development.");
}
