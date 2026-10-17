use super::TestParseContext;
use crate::sem::{
    AnalysisError, CatalogNameLookup, ConflictInfoRead, FullName, InsertArbiter, InsertConflict,
    InsertInferenceModifiers, TableName,
};

impl TestParseContext {
    pub(crate) fn set_conflict_masters(&mut self, masters: &crate::sem::ConflictMasters) {
        self.masters = Some(masters.clone());
    }

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
    fn database_collation(&self) -> Option<&crate::sem::DatabaseCollation> {
        let masters = self.masters.as_ref()?;
        masters.database()
    }
    fn collation_definition(&self, name: &FullName) -> Option<&crate::sem::CollationDefinition> {
        let masters = self.masters.as_ref()?;
        masters.qualified_collation(name)
    }
    fn operator_class_definition(
        &self,
        name: &FullName,
        method: &str,
    ) -> Option<&crate::sem::OperatorClassDefinition> {
        let masters = self.masters.as_ref()?;
        masters.operator_class(name, method)
    }
    fn lookup_sqlite_collation(&self, name: &FullName) -> CatalogNameLookup {
        self.masters
            .as_ref()
            .expect("explicit test namespace")
            .sqlite_lookup(name)
    }
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
