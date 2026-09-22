//! Builder contracts exercised by generated query functions.

#[derive(Debug, bon::Builder)]
pub(crate) struct BonRow {
    id: i64,
    name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MutableRow {
    id: i64,
    name: String,
}

impl MutableRow {
    pub(crate) fn builder() -> MutableBuilder {
        MutableBuilder(Self::default())
    }
}

pub(crate) struct MutableBuilder(MutableRow);

impl MutableBuilder {
    pub(crate) fn id(&mut self, value: i64) {
        self.0.id = value;
    }

    pub(crate) fn name(&mut self, value: String) {
        self.0.name = value;
    }

    pub(crate) fn build(&self) -> MutableRow {
        self.0.clone()
    }
}

#[derive(Debug)]
pub(crate) struct ConsumingRow {
    id: i64,
    name: String,
}

impl ConsumingRow {
    pub(crate) const fn builder() -> Empty {
        Empty
    }
}

pub(crate) struct Empty;
pub(crate) struct WithId(i64);
pub(crate) struct Ready(ConsumingRow);

impl Empty {
    pub(crate) const fn id(self, value: i64) -> WithId {
        WithId(value)
    }
}

impl WithId {
    pub(crate) fn name(self, value: String) -> Ready {
        Ready(ConsumingRow {
            id: self.0,
            name: value,
        })
    }
}

impl Ready {
    pub(crate) fn build(self) -> Result<ConsumingRow, std::io::Error> {
        if self.0.name == "reject" {
            Err(std::io::Error::other("row rejected"))
        } else {
            Ok(self.0)
        }
    }
}

/// An infallible row's query error intentionally has no builder-error conversion.
#[derive(Debug, thiserror::Error)]
pub(crate) enum QueryError {
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
