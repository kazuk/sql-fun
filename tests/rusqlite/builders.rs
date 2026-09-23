//! SQLite-specific fallible builder and query error.

#[derive(Debug, Default, Clone)]
pub(super) struct MutableFallibleRow {
    id: i64,
    name: String,
}

impl MutableFallibleRow {
    pub(super) fn builder() -> MutableFallibleBuilder {
        MutableFallibleBuilder(Self::default())
    }
}

pub(super) struct MutableFallibleBuilder(MutableFallibleRow);

impl MutableFallibleBuilder {
    pub(super) fn id(&mut self, value: i64) {
        self.0.id = value;
    }

    pub(super) fn name(&mut self, value: String) {
        self.0.name = value;
    }

    pub(super) fn build(&self) -> Result<MutableFallibleRow, std::io::Error> {
        if self.0.name == "reject" {
            Err(std::io::Error::other("row rejected"))
        } else {
            Ok(self.0.clone())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum QueryError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("interrupt disconnected")]
    Disconnected,
}

impl QueryError {
    pub(super) const fn interrupt_disconnected() -> Self {
        Self::Disconnected
    }

    pub(super) fn interrupt_handler_cleanup_error<T>(
        error: rusqlite::Error,
        _result: Result<T, Self>,
    ) -> Result<T, Self> {
        Err(Self::Sqlite(error))
    }
}
