use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque, hash_map::Entry},
    path::{Path, PathBuf},
};

use crate::{ExtensionConfigError, ExtensionVersion};

#[derive(Default)]
pub(super) struct ScriptInventory {
    installs: BTreeSet<String>,
    updates: BTreeMap<String, Vec<String>>,
}

impl ScriptInventory {
    pub(super) fn read(directory: &Path, name: &str) -> Result<Self, ExtensionConfigError> {
        let mut inventory = Self::default();
        let prefix = format!("{name}--");
        for entry in directory.read_dir()? {
            let entry = entry?;
            let filename = entry.file_name();
            let Some(filename) = filename.to_str() else {
                continue;
            };
            let Some(stem) = filename.strip_suffix(".sql") else {
                continue;
            };
            let Some(version) = stem.strip_prefix(&prefix) else {
                continue;
            };
            if !entry.path().is_file() {
                continue;
            }
            if let Some((source, target)) = version.split_once("--") {
                if ExtensionVersion::installation(source).is_ok()
                    && ExtensionVersion::installation(target).is_ok()
                {
                    inventory
                        .updates
                        .entry(source.to_owned())
                        .or_default()
                        .push(target.to_owned());
                }
            } else if ExtensionVersion::installation(version).is_ok() {
                inventory.installs.insert(version.to_owned());
            }
        }
        Ok(inventory)
    }

    pub(super) fn available_versions(&self) -> BTreeSet<&str> {
        self.installs
            .iter()
            .flat_map(|start| std::iter::once(start.as_str()).chain(self.routes(start).into_keys()))
            .collect()
    }

    pub(super) fn installation_path<'a>(&'a self, target: &'a str) -> Option<Vec<&'a str>> {
        if self.installs.contains(target) {
            return Some(vec![target]);
        }
        let mut best = None;
        // Equal-length paths prefer the bytewise greatest install version.
        for start in self.installs.iter().rev() {
            let routes = self.routes(start);
            let Some(&(distance, _)) = routes.get(target) else {
                continue;
            };
            if best
                .as_ref()
                .is_some_and(|path: &Vec<&str>| path.len() <= distance + 1)
            {
                continue;
            }
            let mut path = vec![target];
            let mut version = target;
            while let Some(&(_, predecessor)) = routes.get(version) {
                path.push(predecessor);
                version = predecessor;
            }
            path.reverse();
            best = Some(path);
        }
        best
    }

    // Breadth-first search gives the fewest updates. PostgreSQL chooses the
    // bytewise smallest immediate predecessor when equal-length paths meet.
    fn routes<'a>(&'a self, start: &'a str) -> HashMap<&'a str, (usize, &'a str)> {
        let mut routes = HashMap::new();
        let mut queue = VecDeque::from([(start, 0)]);
        while let Some((source, distance)) = queue.pop_front() {
            for target in self.updates.get(source).into_iter().flatten() {
                if self.installs.contains(target) {
                    continue;
                }
                match routes.entry(target.as_str()) {
                    Entry::Vacant(entry) => {
                        entry.insert((distance + 1, source));
                        queue.push_back((target, distance + 1));
                    }
                    Entry::Occupied(mut entry) => {
                        let (current_distance, predecessor) = *entry.get();
                        if current_distance == distance + 1 && source < predecessor {
                            entry.insert((current_distance, source));
                        }
                    }
                }
            }
        }
        routes
    }
}

pub(super) fn script_path(directory: &Path, name: &str, version: &str) -> PathBuf {
    directory.join(format!("{name}--{version}.sql"))
}
