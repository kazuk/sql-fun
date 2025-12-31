use crate::sem::{AlterSequence, AnalysisError, CreateSequence, FullName};

/// get sequence definition
pub trait SequenceInfoRead {
    /// get `CREATE SEQUENCE` statement by full name
    fn get_sequence_impl(&self, key: &FullName) -> Option<&CreateSequence>;
}

/// store and update sequence info
pub trait SequenceInfoWrite
where
    Self: Sized,
{
    /// apply `alter sequence` statement to current schema
    fn apply_alter_sequence(self, alter_seq: &AlterSequence) -> Result<Self, AnalysisError>;

    /// apply `create sequence` statement to current schema
    fn apply_create_sequence(self, create_sequence: &CreateSequence)
    -> Result<Self, AnalysisError>;
}
