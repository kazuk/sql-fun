use crate::sem::{
    AlterSequence, AnalysisError, AnalysisProblem, CreateSequence, FullName, ParseContext,
    SequenceInfoRead, SequenceInfoWrite,
};

use crate::TrieMap;

use super::{SchemaCatalog, SchemaFileContext};

#[derive(Debug, Default)]
pub struct SequenceInfoCollection(TrieMap<CreateSequence>);

impl SequenceInfoCollection {
    fn owned_by(&self, table: &crate::sem::TableName) -> Vec<CreateSequence> {
        self.0
            .iter()
            .filter(|(_, definition)| definition.owned_by() == Some(table))
            .map(|(_, definition)| definition.clone())
            .collect()
    }

    fn get(&self, key: &FullName) -> Option<&CreateSequence> {
        self.0.get(&key.to_string())
    }

    pub(super) fn insert(&mut self, key: &FullName, definition: &CreateSequence) {
        self.0.insert(&key.to_string(), definition.clone());
    }
}

impl SequenceInfoRead for SchemaCatalog {
    fn sequences_owned_by(&self, table: &crate::sem::TableName) -> Vec<CreateSequence> {
        let mut sequences = self.sequences.owned_by(table);
        for inherited in self.base.sequences_owned_by(table) {
            if self.sequences.get(inherited.name()).is_none() {
                sequences.push(inherited);
            }
        }
        sequences
    }

    fn get_sequence_impl(&self, key: &FullName) -> Option<&CreateSequence> {
        if let Some(s) = self.sequences.get(key) {
            Some(s)
        } else {
            self.base.get_sequence_impl(key)
        }
    }
}

impl SequenceInfoWrite for SchemaFileContext {
    fn apply_alter_sequence(mut self, alter_seq: &AlterSequence) -> Result<Self, AnalysisError>
    where
        Self: Sized,
    {
        let seq_name = alter_seq.seq_name();
        let Some((key, mut seq)) =
            crate::sem::search_path::expand_search_path_inst(&self, seq_name, |key| {
                self.get_sequence_impl(key)
                    .cloned()
                    .map(|sequence| (key.clone(), sequence))
            })
        else {
            self.report_analysis_problem(AnalysisProblem::sequence_not_found(seq_name))?;
            return Ok(self);
        };
        seq.apply_alter(alter_seq)?;
        self.catalog.sequences.insert(&key, &seq);
        Ok(self)
    }

    fn apply_create_sequence(
        mut self,
        create_sequence: &CreateSequence,
    ) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_sequence.name())?;
        self.catalog
            .sequences
            .insert(&name, &create_sequence.with_catalog_name(&name));
        Ok(self)
    }
}
