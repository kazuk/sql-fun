use ::notify::{RecommendedWatcher, Watcher};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

impl FileIdentity {
    fn read(path: &Path) -> Result<Self, std::io::Error> {
        let metadata = std::fs::metadata(path)?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[derive(Default)]
struct Entry {
    generation: u64,
    identity: Option<FileIdentity>,
    watched: Option<FileIdentity>,
    hash: Option<String>,
}

#[derive(Default)]
struct State {
    paths: HashMap<PathBuf, Entry>,
}

impl State {
    fn invalidate(&mut self, path: &Path) {
        if let Some(entry) = self.paths.get_mut(path) {
            entry.generation += 1;
            entry.hash = None;
        }
    }

    fn unreliable(&mut self) {
        for entry in self.paths.values_mut() {
            entry.generation += 1;
            entry.hash = None;
            entry.watched = None;
        }
    }
}

/// A generation captured before I/O; late readers cannot restore invalidated data.
#[derive(Clone, Copy)]
pub(crate) struct ReadToken {
    generation: u64,
    identity: FileIdentity,
}

pub(crate) struct FileHashCache {
    state: Arc<Mutex<State>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl FileHashCache {
    pub(crate) fn new() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let weak: Weak<Mutex<State>> = Arc::downgrade(&state);
        let watcher =
            ::notify::recommended_watcher(move |event: ::notify::Result<::notify::Event>| {
                let Some(state) = weak.upgrade() else {
                    return;
                };
                let mut state = state.lock().expect("file hash state");
                match event {
                    Ok(event) => {
                        if event.need_rescan() {
                            state.unreliable();
                        } else if !matches!(event.kind, ::notify::EventKind::Access(_)) {
                            for path in event.paths {
                                state.invalidate(&path);
                            }
                        }
                    }
                    Err(_) => state.unreliable(),
                }
            })
            .ok();
        Self {
            state,
            watcher: Mutex::new(watcher),
        }
    }

    /// The caller supplies only a previously canonicalized, explicitly selected path.
    pub(crate) fn begin(&self, path: &Path) -> Result<ReadToken, std::io::Error> {
        let identity = FileIdentity::read(path)?;
        let mut watcher = self.watcher.lock().expect("file watcher");
        let (generation, needs_watch) = {
            let mut state = self.state.lock().expect("file hash state");
            let entry = state.paths.entry(path.to_path_buf()).or_default();
            let needs_watch = entry.watched != Some(identity);
            if entry.identity != Some(identity) {
                entry.generation += 1;
                entry.hash = None;
                entry.watched = None;
                entry.identity = Some(identity);
            }
            (entry.generation, needs_watch)
        };
        // The watch backend can synchronously dispatch events while servicing
        // watch/unwatch. Never hold callback state while waiting on its worker.
        if needs_watch && let Some(watcher) = watcher.as_mut() {
            let _ = watcher.unwatch(path);
            if watcher
                .watch(path, ::notify::RecursiveMode::NonRecursive)
                .is_ok()
            {
                let mut state = self.state.lock().expect("file hash state");
                let entry = state.paths.get_mut(path).expect("registered path");
                if entry.generation == generation {
                    entry.watched = Some(identity);
                }
            }
        }
        Ok(ReadToken {
            generation,
            identity,
        })
    }

    pub(crate) const fn generation(token: ReadToken) -> u64 {
        token.generation
    }

    /// Only hashes produced by sqlast snapshots are retained here.
    pub(crate) fn lookup(&self, path: &Path, token: ReadToken) -> Option<String> {
        let state = self.state.lock().expect("file hash state");
        if let Some(entry) = state.paths.get(path)
            && entry.generation == token.generation
        {
            entry.hash.clone()
        } else {
            None
        }
    }

    pub(crate) fn populate(
        &self,
        path: &Path,
        token: ReadToken,
        hash: &str,
    ) -> Result<(), std::io::Error> {
        let identity = FileIdentity::read(path)?;
        let mut state = self.state.lock().expect("file hash state");
        if let Some(entry) = state.paths.get_mut(path)
            && identity == token.identity
            && entry.watched == Some(identity)
            && entry.generation == token.generation
        {
            entry.hash = Some(hash.to_owned());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn invalidate(&self, path: &Path) {
        self.state.lock().expect("file hash state").invalidate(path);
    }
}

#[cfg(test)]
mod tests {
    use super::FileHashCache;

    #[test]
    fn missing_watcher_does_not_split_identical_inflight_loads() {
        let file = ::tempfile::NamedTempFile::new().unwrap();
        let path = file.path().canonicalize().unwrap();
        let cache = FileHashCache::new();
        *cache.watcher.lock().unwrap() = None;
        let first = cache.begin(&path).unwrap();
        let second = cache.begin(&path).unwrap();
        assert_eq!(first.generation, second.generation);
        cache.populate(&path, first, "snapshot hash").unwrap();
        assert!(
            cache
                .state
                .lock()
                .unwrap()
                .paths
                .get(&path)
                .unwrap()
                .hash
                .is_none()
        );
    }

    // Removed native_notification_invalidates_in_place_change; restore from df446be6c1d950353fbd7f79916f79c68bfd396d.
    #[test]
    fn inode_replacement_invalidates_without_notification() {
        let directory = ::tempfile::tempdir().unwrap();
        let path = directory.path().join("dump.sql");
        std::fs::write(&path, "old").unwrap();
        let cache = FileHashCache::new();
        let token = cache.begin(&path).unwrap();
        cache.populate(&path, token, "old snapshot hash").unwrap();
        assert_eq!(
            cache.lookup(&path, token).as_deref(),
            Some("old snapshot hash")
        );
        let replacement = directory.path().join("replacement.sql");
        std::fs::write(&replacement, "new").unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert!(cache.lookup(&path, cache.begin(&path).unwrap()).is_none());
    }
}
