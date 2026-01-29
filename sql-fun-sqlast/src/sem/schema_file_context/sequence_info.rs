use crate::{
    TrieMap,
    sem::{
        AlterSequence, AnalysisError, AnalysisProblem, BaseParseContext, CreateSequence, FullName,
        SequenceInfoRead, SequenceInfoWrite,
    },
};

use super::SchemaFileContext;

#[derive(Debug, Default)]
pub struct SequenceInfoCollection(TrieMap<CreateSequence>);

impl SequenceInfoCollection {
    fn get(&self, key: &FullName) -> Option<&CreateSequence> {
        self.0.get(&key.to_string())
    }

    fn insert(&mut self, key: &FullName, definition: &CreateSequence) {
        self.0.insert(&key.to_string(), definition.clone());
    }
}

impl<TBaseContext> SequenceInfoRead for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn get_sequence_impl(&self, key: &FullName) -> Option<&CreateSequence> {
        if let Some(s) = self.sequences.get(key) {
            Some(s)
        } else {
            self.base.get_sequence_impl(key)
        }
    }
}

impl<TBaseContext> SequenceInfoWrite for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn apply_alter_sequence(mut self, alter_seq: &AlterSequence) -> Result<Self, AnalysisError>
    where
        Self: Sized,
    {
        let seq_name = alter_seq.seq_name();
        let Some(mut seq) = self.get_sequence(seq_name).cloned() else {
            self.report_problem(AnalysisProblem::sequence_not_found(seq_name))?;
            return Ok(self);
        };

        seq.apply_alter(alter_seq)?;
        self.sequences.insert(seq_name, &seq);
        Ok(self)
    }

    fn apply_create_sequence(
        mut self,
        create_sequence: &CreateSequence,
    ) -> Result<Self, AnalysisError> {
        let name = create_sequence.name();
        self.sequences.insert(name, create_sequence);
        Ok(self)
    }
}
