#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{QualifiedName, Stmt};
use crate::{engine_specific::TursoSequenceFeature, sem::AnalysisError};

/// CREATE SEQUENCE syntax, including options omitted by the caller.
#[derive(Debug, derive_getters::Getters)]
pub struct CreateSequenceStatement {
    #[getter(copy)]
    if_not_exists: bool,
    #[getter(skip)]
    seq_name: QualifiedName,
    #[getter(copy)]
    start: Option<i64>,
    #[getter(copy)]
    increment: Option<i64>,
    #[getter(copy)]
    min_value: Option<i64>,
    #[getter(copy)]
    max_value: Option<i64>,
    #[getter(copy)]
    cycle: bool,
}

impl CreateSequenceStatement {
    /// Sequence name, reporting use of Turso sequence syntax.
    pub fn seq_name<Gate: TursoSequenceFeature>(
        &self,
        gate: &mut Gate,
    ) -> Result<&QualifiedName, AnalysisError> {
        gate.create_sequence_used(self)?;
        Ok(&self.seq_name)
    }
}

/// DROP SEQUENCE syntax.
#[derive(Debug, derive_getters::Getters)]
pub struct DropSequenceStatement {
    #[getter(copy)]
    if_exists: bool,
    #[getter(skip)]
    seq_name: QualifiedName,
}

impl DropSequenceStatement {
    /// Sequence name, reporting use of Turso sequence syntax.
    pub fn seq_name<Gate: TursoSequenceFeature>(
        &self,
        gate: &mut Gate,
    ) -> Result<&QualifiedName, AnalysisError> {
        gate.drop_sequence_used(self)?;
        Ok(&self.seq_name)
    }
}

impl Stmt {
    /// Create a sequence statement without assigning defaults to omitted options.
    pub fn create_sequence(
        if_not_exists: bool,
        seq_name: &QualifiedName,
        start: Option<i64>,
        increment: Option<i64>,
        min_value: Option<i64>,
        max_value: Option<i64>,
        cycle: bool,
    ) -> Self {
        Self::CreateSequence(CreateSequenceStatement {
            if_not_exists,
            seq_name: seq_name.clone(),
            start,
            increment,
            min_value,
            max_value,
            cycle,
        })
    }

    /// Create a drop-sequence statement.
    pub fn drop_sequence(if_exists: bool, seq_name: &QualifiedName) -> Self {
        Self::DropSequence(DropSequenceStatement {
            if_exists,
            seq_name: seq_name.clone(),
        })
    }
}

#[cfg(test)]
mod test_sequence_conversion {
    use crate::syn::sqlite::sequence_statements::{
        CreateSequenceStatement, DropSequenceStatement, Stmt, TursoSequenceFeature,
    };
    #[cfg(feature = "sqlite")]
    use crate::{sem::AnalysisError, test_helpers::SqliteSynBuilder};

    #[derive(Default)]
    struct Gate {
        creates: usize,
        drops: usize,
    }
    impl TursoSequenceFeature for Gate {
        fn create_sequence_used(
            &mut self,
            _: &CreateSequenceStatement,
        ) -> Result<(), AnalysisError> {
            self.creates += 1;
            Ok(())
        }
        fn drop_sequence_used(&mut self, _: &DropSequenceStatement) -> Result<(), AnalysisError> {
            self.drops += 1;
            Ok(())
        }
    }

    #[test]
    fn preserves_all_options_and_reports_engine_specific_use() {
        let Stmt::CreateSequence(sequence) = SqliteSynBuilder.parse_as_stmt(
            "CREATE SEQUENCE IF NOT EXISTS [main].[counter] START WITH -1 INCREMENT BY -2 MINVALUE -100 MAXVALUE 10 CYCLE"
        ) else { panic!("expected sequence") };
        assert!(sequence.if_not_exists());
        assert_eq!(sequence.start(), Some(-1));
        assert_eq!(sequence.increment(), Some(-2));
        assert_eq!(sequence.min_value(), Some(-100));
        assert_eq!(sequence.max_value(), Some(10));
        assert!(sequence.cycle());
        let mut gate = Gate::default();
        let name = sequence.seq_name(&mut gate).unwrap();
        assert_eq!(name.name().value(), "counter");
        assert_eq!(name.db_name().unwrap().value(), "main");
        assert_eq!(*name.name().quote(), Some('['));
        assert_eq!(gate.creates, 1);
        let Stmt::DropSequence(drop) =
            SqliteSynBuilder.parse_as_stmt("DROP SEQUENCE IF EXISTS [main].[counter]")
        else {
            panic!("expected drop sequence")
        };
        assert!(drop.if_exists());
        assert_eq!(drop.seq_name(&mut gate).unwrap().name().value(), "counter");
        assert_eq!(gate.drops, 1);
    }

    struct Reject;
    impl TursoSequenceFeature for Reject {
        fn create_sequence_used(
            &mut self,
            _: &CreateSequenceStatement,
        ) -> Result<(), AnalysisError> {
            Err(std::io::Error::other("sequence denied").into())
        }
        fn drop_sequence_used(&mut self, _: &DropSequenceStatement) -> Result<(), AnalysisError> {
            Err(std::io::Error::other("sequence denied").into())
        }
    }

    #[test]
    fn propagates_engine_hook_errors_after_parsing() {
        let Stmt::CreateSequence(sequence) =
            SqliteSynBuilder.parse_as_stmt("CREATE SEQUENCE counter")
        else {
            panic!("expected sequence")
        };
        assert_eq!(
            sequence.seq_name(&mut Reject).unwrap_err().to_string(),
            "sequence denied"
        );
        let Stmt::DropSequence(sequence) = SqliteSynBuilder.parse_as_stmt("DROP SEQUENCE counter")
        else {
            panic!("expected sequence")
        };
        assert_eq!(
            sequence.seq_name(&mut Reject).unwrap_err().to_string(),
            "sequence denied"
        );
    }

    #[test]
    fn preserves_omitted_options_and_no_cycle() {
        for sql in [
            "CREATE SEQUENCE counter",
            "CREATE SEQUENCE counter NO CYCLE",
        ] {
            let Stmt::CreateSequence(sequence) = SqliteSynBuilder.parse_as_stmt(sql) else {
                panic!("expected sequence")
            };
            assert!(!sequence.if_not_exists());
            assert_eq!(sequence.start(), None);
            assert_eq!(sequence.increment(), None);
            assert_eq!(sequence.min_value(), None);
            assert_eq!(sequence.max_value(), None);
            assert!(!sequence.cycle());
        }
        let Stmt::DropSequence(drop) = SqliteSynBuilder.parse_as_stmt("DROP SEQUENCE counter")
        else {
            panic!("expected drop")
        };
        assert!(!drop.if_exists());
    }
}
