use crate::lockfile::{
    LockfileDocument, LockfileEntry, LockfileStatement, QueryParameter, ResultSetField,
};

pub(super) fn entry(name: &str, count: usize) -> LockfileEntry {
    let statements = (0..count)
        .map(|index| {
            LockfileStatement::new(
                &format!("SELECT ${}", index + 1),
                &[QueryParameter::new(
                    &format!("binding_{index}"),
                    u16::try_from(index + 1).unwrap(),
                )],
                &[ResultSetField::new(0, &format!("column_{index}"), "int4")],
            )
        })
        .collect::<Vec<_>>();
    LockfileEntry::new(name, name, &statements).unwrap()
}

pub(super) struct Fixture {
    directory: ::tempfile::TempDir,
}

impl Fixture {
    pub(super) fn new() -> Self {
        Self {
            directory: ::tempfile::tempdir().unwrap(),
        }
    }
    pub(super) fn path(&self) -> std::path::PathBuf {
        self.directory.path().join("queries.lock")
    }
    pub(super) fn write(&self, document: &LockfileDocument) -> String {
        let content = document.encode().unwrap();
        LockfileDocument::replace(&self.path(), &content).unwrap();
        content
    }
    pub(super) fn write_raw(&self, content: &str) {
        std::fs::write(self.path(), content).unwrap();
    }
    pub(super) fn content(&self) -> String {
        std::fs::read_to_string(self.path()).unwrap()
    }
}
