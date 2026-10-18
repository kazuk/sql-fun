use crate::sem::{
    AlterTable, AnalysisError, AnalysisProblem, CreateTable, DropTable, FullName, ImplicitChange,
    ParseContext, TableInfoRead, TableInfoWrite, TableName, search_path::expand_search_path_inst,
};

use crate::TrieMap;

use super::{SchemaCatalog, SchemaFileContext};

#[derive(Debug, Default)]
pub struct TableDefinitionCollection(TrieMap<CreateTable>, Vec<FullName>);

impl TableDefinitionCollection {
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

    fn apply_alter(
        &mut self,
        alter_table: &AlterTable,
        key: &FullName,
        mut table: CreateTable,
    ) -> Result<Vec<ImplicitChange>, AnalysisError> {
        let implicit_changes = table.apply_alter(alter_table)?;
        self.insert_create_table(key, &table);
        Ok(implicit_changes)
    }

    fn push_drop_statement(
        &mut self,
        key: &FullName,
        mut table: CreateTable,
        drop_table: &DropTable,
    ) {
        table.push_drop_statement(drop_table);
        self.insert_create_table(key, &table);
    }
}

impl TableInfoRead for SchemaCatalog {
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable> {
        if let Some(v) = self.tables.get(key) {
            Some(v)
        } else {
            self.base.get_table_impl(key)
        }
    }
}

impl SchemaFileContext {
    fn table_for_update(&self, name: &TableName) -> Option<(FullName, CreateTable)> {
        expand_search_path_inst(self, name.full_name(), |key| {
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
        let table_name = alter_table.table();
        let Some((key, table)) = self.table_for_update(table_name) else {
            self.report_analysis_problem(AnalysisProblem::table_not_found(table_name))?;
            return Ok(self);
        };

        let implicit_changes = self.catalog.tables.apply_alter(alter_table, &key, table)?;
        if let Some(owner) = alter_table.applied_owner() {
            self.change_dependent_sequence_owners(&TableName::from(key), owner);
        }
        let mut context = self;
        for implicit_change in implicit_changes {
            context = context.apply_implicit_change(implicit_change)?;
        }
        Ok(context)
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
            self.catalog
                .tables
                .push_drop_statement(&key, table, drop_table);
        }
        Ok(self)
    }

    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable) {
        self.catalog.tables.insert_create_table(name, create_table);
    }
}

#[cfg(test)]
mod test_drop_catalog_key {
    use crate::sem::{
        BaseParseContext, DropTable, FullName, ParseContext, TableInfoRead, TableInfoWrite,
        TableName,
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
        use crate::sem::{
            AnalysisError, CreateTable, CreateTableMode, FullName, ParseContext, SchemaFileContext,
            SchemaName, TableName,
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
