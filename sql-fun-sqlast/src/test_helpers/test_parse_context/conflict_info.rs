use super::TestParseContext;
use crate::sem::{
    AnalysisError, CatalogNameLookup, ConflictInfoRead, FullName, InsertArbiter, InsertConflict,
    InsertInferenceModifiers, TableName,
};

impl TestParseContext {
    pub(crate) fn expect_conflict_target(&mut self, target: &TableName) {
        self.expected_conflict_target = Some(target.clone());
    }
    pub(crate) fn set_collation_lookup(&mut self, name: &FullName, result: &CatalogNameLookup) {
        self.collation_results.insert(name.clone(), result.clone());
    }
    pub(crate) fn set_operator_class_lookup(
        &mut self,
        name: &FullName,
        result: &CatalogNameLookup,
    ) {
        self.operator_class_results
            .insert((name.clone(), "btree".to_owned()), result.clone());
    }
    pub(crate) fn expect_conflict_modifiers(
        &mut self,
        modifiers: &[InsertInferenceModifiers],
        arbiters: &[InsertArbiter],
    ) {
        self.expected_conflict_modifiers = Some(modifiers.to_vec());
        self.conflict_arbiters = arbiters.to_vec();
    }
}
impl ConflictInfoRead for TestParseContext {
    fn lookup_collation(&self, name: &FullName) -> CatalogNameLookup {
        self.collation_results
            .get(name)
            .cloned()
            .unwrap_or_else(|| CatalogNameLookup::missing(name))
    }
    fn lookup_operator_class(&self, name: &FullName, method: &str) -> CatalogNameLookup {
        self.operator_class_results
            .get(&(name.clone(), method.to_owned()))
            .cloned()
            .unwrap_or_else(|| CatalogNameLookup::missing(name))
    }
    fn infer_conflict_arbiters(
        &self,
        table: &TableName,
        conflict: &InsertConflict,
    ) -> Result<Vec<InsertArbiter>, AnalysisError> {
        if let Some(expected) = &self.expected_conflict_modifiers {
            if let Some(expected) = &self.expected_conflict_target {
                assert_eq!(table, expected);
            } else {
                assert_eq!(table.full_name(), &FullName::with_schema("public", "items"));
            }
            assert_eq!(
                conflict.modifiers(),
                expected,
                "bound modifiers supplied to catalog inference"
            );
            Ok(self.conflict_arbiters.clone())
        } else {
            crate::sem::infer_from_catalog(self, table, conflict)
        }
    }
}
