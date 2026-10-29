use crate::context::schema_file_context::{SchemaCatalog, SchemaFileContext};
use crate::{
    TrieMap,
    context::{ParseContext, TableInfoRead, TableInfoWrite},
    sem::{
        AlterTable, AnalysisError, AnalysisProblem, CreateTable, FullName, TableName,
        search_path::expand_relation_search_path,
    },
};

#[derive(Debug, Default)]
pub struct TableDefinitionCollection(TrieMap<CreateTable>, Vec<FullName>);

impl TableDefinitionCollection {
    pub(super) fn append_names(&self, names: &mut Vec<FullName>) {
        for name in &self.1 {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }

    pub(super) fn sqlite_name(&self, name: &FullName) -> Option<FullName> {
        self.1
            .iter()
            .find(|candidate| candidate.sqlite_eq(name))
            .cloned()
    }

    pub(super) fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable) {
        self.0.insert(&name.to_string(), create_table.clone());
        if !self.1.contains(name) {
            self.1.push(name.clone());
        }
    }

    fn get(&self, name: &FullName) -> Option<&CreateTable> {
        self.0.get(&name.to_string())
    }
}

impl TableInfoRead for SchemaCatalog {
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable> {
        let identity = self.relation_identity(key);
        let key = &identity;
        if self.renamed_tables.contains(key) {
            return None;
        }
        if let Some(v) = self.tables.get(key) {
            Some(v)
        } else {
            self.base.get_table_impl(key)
        }
    }
}

impl SchemaFileContext {
    fn table_for_update(&self, name: &TableName) -> Option<(FullName, CreateTable)> {
        expand_relation_search_path(self, name.full_name(), |key| {
            let table = self.get_table_impl(key)?.clone();
            // A declaration may be unqualified even when its catalog key is
            // qualified. Retain the key that actually matched the lookup.
            Some((key.clone(), table))
        })
    }
}

impl TableInfoWrite for SchemaFileContext {
    #[tracing::instrument(skip(self))]
    fn apply_alter_table(mut self, alter_table: &AlterTable) -> Result<Self, AnalysisError> {
        if !alter_table.can_apply() {
            return Ok(self);
        }
        if let Some(destination) = alter_table.rename_destination() {
            return self.apply_table_rename(alter_table, destination);
        }
        let table_name = alter_table.table();
        let bound_table = alter_table.resolved_target().map(|target| {
            self.get_table_impl(target.full_name())
                .cloned()
                .map(|table| (target.full_name().clone(), table))
        });
        let table = bound_table.unwrap_or_else(|| self.table_for_update(table_name));
        let Some((key, mut table)) = table else {
            self.report_analysis_problem(AnalysisProblem::table_not_found(table_name))?;
            return Ok(self);
        };

        if let Some(name) = alter_table.conflicting_constraint(&table) {
            self.report_analysis_problem(AnalysisProblem::creation(
                crate::sem::problem::CreationProblem::constraint(
                    &key,
                    &FullName::from_local_name(&name),
                    &self.creation_span(),
                ),
            ))?;
            return Ok(self);
        }
        let implicit_changes = table.apply_alter(alter_table)?;
        let mut indexes = Vec::new();
        for change in implicit_changes {
            let index = change.into_index().with_table_identity(&key);
            if !self.permits_relation_creation(
                index.name(),
                crate::sem::Kind::index(),
                false,
                false,
            )? {
                return Ok(self);
            }
            if crate::sem::CreateIndex::contains_name(&indexes, index.name()) {
                self.report_analysis_problem(AnalysisProblem::creation(
                    crate::sem::problem::CreationProblem::relation(
                        index.name(),
                        crate::sem::Kind::index(),
                        crate::sem::Kind::index(),
                        &self.creation_span(),
                    ),
                ))?;
                return Ok(self);
            }
            indexes.push(index);
        }
        self.catalog.store_table(&key, &table);
        if let Some(owner) = alter_table.applied_owner() {
            self.change_dependent_sequence_owners(&TableName::from(key), owner);
        }
        for index in indexes {
            self.catalog.store_index(&index);
        }
        Ok(self)
    }

    fn apply_drop_table(
        mut self,
        drop_table: &crate::sem::DropTable,
    ) -> Result<Self, AnalysisError> {
        let table_names = drop_table.table_names();
        for table_name in table_names {
            let Some((key, table)) = self.table_for_update(table_name) else {
                self.report_analysis_problem(AnalysisProblem::table_not_found(table_name))?;
                continue;
            };
            let mut table = table;
            table.push_drop_statement(drop_table);
            self.catalog.store_table(&key, &table);
        }
        Ok(self)
    }

    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable) {
        self.catalog.store_table(name, create_table);
    }
}

#[cfg(test)]
mod test_drop_catalog_key {
    use crate::{
        context::{BaseParseContext, ParseContext, TableInfoRead, TableInfoWrite},
        sem::{DropTable, FullName, TableName},
    };

    #[rstest::rstest]
    #[case::qualified("a.items", &["b", "a"], "a", "b")]
    #[case::unqualified("items", &["a", "b"], "a", "b")]
    #[case::reversed("items", &["b", "a"], "b", "a")]
    fn drop_copies_unqualified_declarations_under_the_resolved_catalog_key(
        #[case] target: &str,
        #[case] path: &[&str],
        #[case] selected: &str,
        #[case] other: &str,
        #[values(false, true)] inherited: bool,
    ) -> testresult::TestResult {
        let mut context = test_helpers::unqualified_tables()?;
        if inherited {
            let base = context.freeze();
            context = crate::test_helpers::schema_layer(std::path::Path::new("child.sql"), &base);
        }
        test_helpers::set_search_path(&mut context, path);
        let name = TableName::from(target.parse::<FullName>()?);
        let context = context.apply_drop_table(&DropTable::new(&[name], false, false))?;

        // DROP records a statement without removing the table. Verify that its
        // local copy shadows the resolved key, not the unqualified declaration.
        let key = FullName::with_schema(selected, "items");
        assert!(context.catalog.tables.get(&key).is_some());
        assert!(
            context
                .catalog
                .tables
                .get(&"items".parse::<FullName>()?)
                .is_none()
        );
        assert!(
            context
                .get_table_impl(&FullName::with_schema(other, "items"))
                .is_some()
        );
        assert!(context.get_table(&TableName::from(key)).is_some());
        Ok(())
    }

    mod test_helpers {
        use crate::{
            context::{ParseContext, SchemaFileContext},
            sem::{AnalysisError, CreateTable, CreateTableMode, FullName, SchemaName, TableName},
        };

        pub(super) fn unqualified_tables() -> Result<SchemaFileContext, AnalysisError> {
            let root = crate::test_helpers::TestParseContext::default().freeze();
            let mut context =
                crate::test_helpers::schema_layer(std::path::Path::new("parent.sql"), &root);
            let table = CreateTable::new(
                &TableName::from("items".parse::<FullName>()?),
                Vec::new(),
                CreateTableMode::from_if_not_exists(false),
                false,
                &crate::test_helpers::source_span(),
            );
            for schema in ["a", "b"] {
                context.set_search_path(&[SchemaName::from(schema)]);
                context = context.apply_create_table(&table)?;
            }
            Ok(context)
        }

        pub(super) fn set_search_path(context: &mut SchemaFileContext, path: &[&str]) {
            let mut schemas = Vec::new();
            for schema in path {
                schemas.push(SchemaName::from(schema));
            }
            context.set_search_path(&schemas);
        }
    }
}
