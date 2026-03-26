use crate::syn::sqlite::Name;

/// begin transaction statement
#[derive(derive_getters::Getters)]
pub struct BeginStatement {
    transaction_type: Option<TransactionType>,
    name: Option<Name>,
}

impl BeginStatement {
    /// create instance
    pub fn new(transaction_type: Option<TransactionType>, name: Option<Name>) -> Self {
        Self {
            transaction_type,
            name,
        }
    }
}

/// commit statement
#[derive(derive_getters::Getters)]
pub struct CommitStatement {
    name: Option<Name>,
}

impl CommitStatement {
    /// create instance
    pub fn new(name: Option<Name>) -> Self {
        Self { name }
    }
}

/// rollback statement
#[derive(derive_getters::Getters)]
pub struct RollbackStatement {
    tx_name: Option<Name>,
    savepoint_name: Option<Name>,
}

impl RollbackStatement {
    /// create instance
    pub fn new(tx_name: Option<Name>, savepoint_name: Option<Name>) -> Self {
        Self {
            tx_name,
            savepoint_name,
        }
    }
}

/// savepoint statement
#[derive(derive_getters::Getters)]
pub struct SavepointStatement {
    name: Name,
}

impl SavepointStatement {
    /// create instance
    pub fn new(name: Name) -> Self {
        Self { name }
    }
}

/// release transaction statement
#[derive(derive_getters::Getters)]
pub struct ReleaseStatement {
    name: Name,
}

impl ReleaseStatement {
    /// create instance
    pub fn new(name: Name) -> Self {
        Self { name }
    }
}

use turso_parser::ast::TransactionType as TransactionTypeIn;

/// transaction type
pub enum TransactionType {
    /// deferred
    Deferred,
    /// immediate
    Immediate,
    /// exclusive
    Exclusive,
    /// concurrent
    Concurrent,
}

impl From<TransactionTypeIn> for TransactionType {
    fn from(value: TransactionTypeIn) -> Self {
        match value {
            TransactionTypeIn::Deferred => Self::Deferred,
            TransactionTypeIn::Immediate => Self::Immediate,
            TransactionTypeIn::Exclusive => Self::Exclusive,
            TransactionTypeIn::Concurrent => Self::Concurrent,
        }
    }
}
