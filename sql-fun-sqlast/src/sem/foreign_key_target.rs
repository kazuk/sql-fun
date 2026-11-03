//! Creation-time foreign-key bindings, independent of subsequent search paths.
use crate::{
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{AnalysisError, AnalysisProblem, TypeReference},
};

/// A completed target lookup: `None` means the relation was unresolved.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct ForeignKeyTarget(Option<TableName>);

impl ForeignKeyTarget {
    pub(crate) fn new(canonical: Option<&TableName>) -> Self {
        Self(canonical.cloned())
    }

    pub(crate) const fn relation(&self) -> Option<&TableName> {
        self.0.as_ref()
    }

    fn canonical_relation(context: &impl ParseContext, name: TableName) -> TableName {
        if context.dialect() == crate::ast::Dialect::sqlite() {
            TableName::from(name.full_name().sqlite_identity())
        } else {
            name
        }
    }

    fn resolve(context: &impl ParseContext, requested: &TableName) -> Self {
        Self(
            context
                .resolve_table_name(requested)
                .map(|name| Self::canonical_relation(context, name)),
        )
    }

    pub(crate) fn resolve_relation(
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<Self, AnalysisError> {
        let target = Self::resolve(context, requested);
        if target.relation().is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
        }
        Ok(target)
    }

    pub(crate) fn resolve_declared_reference(
        context: &mut impl ParseContext,
        requested: &TableName,
        explicit: Option<&[ColumnName]>,
        pending: &crate::sem::CreateTable,
    ) -> Result<(Self, Option<Vec<ColumnName>>, bool), AnalysisError> {
        let identity = context.creation_name(pending.name().full_name(), pending.temporary())?;
        let lookup = if context.dialect() == crate::ast::Dialect::sqlite() {
            let Some(schema) = identity.schema() else {
                AnalysisError::raise_unexpected_none("SQLite creation schema")?;
                AnalysisError::never();
            };
            requested.full_name().replace_schema(schema)
        } else {
            requested.full_name().clone()
        };
        let canonical =
            crate::sem::search_path::expand_relation_search_path(context, &lookup, |key| {
                let matches = if context.dialect() == crate::ast::Dialect::sqlite() {
                    key.sqlite_eq(&identity)
                } else {
                    key == &identity
                };
                if matches {
                    Some(TableName::from(identity.clone()))
                } else {
                    context.resolve_table_name(&TableName::from(key.clone()))
                }
            });
        let target = Self(canonical.map(|name| Self::canonical_relation(context, name)));
        if target.relation().is_none() && context.dialect() == crate::ast::Dialect::postgres() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
        }
        let (columns, valid) = if target.references_identity(context, &identity) {
            let (columns, valid) =
                pending.resolve_reference_columns(context, requested, explicit)?;
            (Some(columns), valid)
        } else {
            target.referenced_columns(context, requested, explicit)?
        };
        Ok((target, columns, valid))
    }

    fn references_identity(
        &self,
        context: &impl ParseContext,
        identity: &crate::names::FullName,
    ) -> bool {
        self.relation().is_some_and(|name| {
            if context.dialect() == crate::ast::Dialect::sqlite() {
                name.full_name().sqlite_eq(identity)
            } else {
                name.full_name() == identity
            }
        })
    }

    pub(crate) fn resolve_column(
        context: &mut impl ParseContext,
        requested: &TableName,
        column: &ColumnName,
    ) -> Result<(Self, Option<TypeReference>), AnalysisError> {
        let target = Self::resolve(context, requested);
        let Some(relation) = target.relation() else {
            context.report_analysis_problem(AnalysisProblem::table_not_found(requested))?;
            return Ok((target, None));
        };
        let definition = context.get_table_impl(relation.full_name());
        let column_type = if let Some(table) = definition
            && let Some(column) = table.get_column_def(column)
        {
            column.get_type_cloned()
        } else {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                requested, column,
            ))?;
            None
        };
        Ok((target, column_type))
    }

    pub(crate) fn referenced_columns(
        &self,
        context: &mut impl ParseContext,
        requested: &TableName,
        explicit: Option<&[ColumnName]>,
    ) -> Result<(Option<Vec<ColumnName>>, bool), AnalysisError> {
        let definition = if let Some(name) = self.relation() {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        if let Some(definition) = definition {
            let (columns, valid) =
                definition.resolve_reference_columns(context, requested, explicit)?;
            Ok((Some(columns), valid))
        } else {
            Ok((
                explicit.map(<[ColumnName]>::to_vec),
                context.dialect() == crate::ast::Dialect::sqlite(),
            ))
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(relation) = &mut self.0 {
            relation.rename_reference(old, new);
        }
    }
}

#[cfg(test)]
mod test_foreign_key_bindings {
    use crate::{
        context::{ParseContext, TableInfoRead},
        names::{ColumnName, FullName},
        test_helpers::foreign_keys as fk,
    };

    #[rstest::rstest]
    fn search_path_changes_do_not_rebind_existing_foreign_keys(
        #[values(
            fk::Declaration::inline(),
            fk::Declaration::create(),
            fk::Declaration::alter()
        )]
        declaration: fk::Declaration,
    ) {
        let mut context = declaration.bind(fk::parents(), "child", "parent(id)");
        assert!(context.problems().is_empty());
        assert_eq!(
            declaration.target(&context, "child"),
            Some(fk::name("a", "parent"))
        );
        fk::reversed_path(&mut context);
        let context = declaration.bind(context, "later", "parent(id)");
        assert_eq!(
            declaration.target(&context, "later"),
            Some(fk::name("b", "parent"))
        );
        let parent = context.freeze();
        let context = fk::apply(fk::child(&parent), "ALTER TABLE b.parent RENAME TO other");
        assert_eq!(
            declaration.target(&context, "child"),
            Some(fk::name("a", "parent"))
        );
        assert_eq!(
            declaration.target(&context, "later"),
            Some(fk::name("b", "other"))
        );
        let context = fk::apply(context, "ALTER TABLE a.parent RENAME TO renamed");
        assert_eq!(
            declaration.target(&context, "child"),
            Some(fk::name("a", "renamed"))
        );
        assert_eq!(
            declaration.target(&parent, "child"),
            Some(fk::name("a", "parent"))
        );
        assert_eq!(
            declaration.target(&parent, "later"),
            Some(fk::name("b", "parent"))
        );
        assert!(context.problems().is_empty());
        let context = fk::apply(context, "SELECT id FROM a.child");
        assert!(context.problems().is_empty());
    }

    #[rstest::rstest]
    #[case::qualified("a.parent(id)", "a", "parent")]
    #[case::quoted(r#"a."Parent"(id)"#, "a", "Parent")]
    fn explicit_and_quoted_targets_remain_exact(
        #[case] reference: &str,
        #[case] schema: &str,
        #[case] local: &str,
        #[values(
            fk::Declaration::inline(),
            fk::Declaration::create(),
            fk::Declaration::alter()
        )]
        declaration: fk::Declaration,
    ) {
        let mut context = fk::apply(
            fk::parents(),
            r#"CREATE TABLE a."Parent"(id integer PRIMARY KEY)"#,
        );
        fk::reversed_path(&mut context);
        let context = declaration.bind(context, "child", reference);
        assert!(context.problems().is_empty());
        assert_eq!(
            declaration.target(&context, "child"),
            Some(fk::name(schema, local))
        );
    }

    #[rstest::rstest]
    fn binding_uses_the_catalog_key_not_the_stored_declaration(
        #[values(
            fk::Declaration::inline(),
            fk::Declaration::create(),
            fk::Declaration::alter()
        )]
        declaration: fk::Declaration,
    ) {
        let context = fk::apply(fk::context(), "CREATE TABLE parent(id integer PRIMARY KEY)");
        let definition = context
            .get_table_impl(&FullName::with_schema("a", "parent"))
            .unwrap();
        assert!(definition.name().full_name().schema().is_none());
        let context = declaration.bind(context, "child", "parent(id)");
        assert_eq!(
            declaration.target(&context, "child"),
            Some(fk::name("a", "parent"))
        );
        assert!(context.problems().is_empty());
    }

    #[rstest::rstest]
    #[case::missing_relation("missing(id)", None, "table_not_found", "relation_not_found")]
    #[case::missing_column(
        "parent(absent)",
        Some("parent"),
        "column_not_found_in_table",
        "column_not_found_in_table"
    )]
    fn failed_lookup_is_retained_without_retroactive_resolution(
        #[case] reference: &str,
        #[case] resolved: Option<&str>,
        #[case] inline_code: &str,
        #[case] table_code: &str,
        #[values(fk::Declaration::inline(), fk::Declaration::alter())] declaration: fk::Declaration,
    ) {
        let mut context = declaration.bind(fk::parents(), "child", reference);
        assert_eq!(
            declaration.target(&context, "child"),
            resolved.map(|name| fk::name("a", name))
        );
        assert_eq!(context.problems().len(), 1);
        let expected_code = if declaration.is_table() {
            table_code
        } else {
            inline_code
        };
        assert_eq!(context.problems()[0].code(), expected_code);
        fk::reversed_path(&mut context);
        let context = fk::apply(context, "CREATE TABLE b.missing(id integer PRIMARY KEY)");
        let context = fk::apply(context, "ALTER TABLE b.missing RENAME TO appeared");
        assert_eq!(
            declaration.target(&context, "child"),
            resolved.map(|name| fk::name("a", name))
        );
        let context = fk::apply(context, "SELECT id FROM a.child");
        assert_eq!(context.problems().len(), 1);
        let parent = context.freeze();
        let context = fk::child(&parent);
        assert!(context.problems().is_empty());
        assert_eq!(
            declaration.target(&context, "child"),
            resolved.map(|name| fk::name("a", name))
        );
    }

    #[rstest::rstest]
    fn relation_shadowing_does_not_bind_a_table_in_a_later_schema(
        #[values(fk::Declaration::inline(), fk::Declaration::alter())] declaration: fk::Declaration,
    ) {
        let context = fk::apply(
            fk::context(),
            "CREATE TABLE b.parent(id integer PRIMARY KEY)",
        );
        let context = fk::apply(context, "CREATE VIEW a.parent AS SELECT 1 AS id");
        let context = declaration.bind(context, "child", "parent(id)");
        assert!(declaration.target(&context, "child").is_none());
        assert_eq!(context.problems().len(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_target_identity_follows_case_insensitive_table_rename() -> testresult::TestResult {
        let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE Parent(id PRIMARY KEY)");
        let context = fk::apply_sqlite(context, "CREATE TABLE child(id REFERENCES PARENT(id))");
        let constraint = fk::inline(&context, &FullName::with_schema("main", "child"));
        assert_eq!(
            constraint.primary_key_table(),
            Some(&fk::name("main", "parent"))
        );
        let parent = context.freeze();
        let context = fk::apply_sqlite(fk::child(&parent), "ALTER TABLE pArEnT RENAME TO renamed");
        let constraint = fk::inline(&context, &FullName::with_schema("main", "child"));
        assert_eq!(
            constraint.primary_key_table(),
            Some(&fk::name("main", "renamed"))
        );
        assert!(context.problems().is_empty());
        assert_eq!(
            fk::inline(&parent, &FullName::with_schema("main", "child")).primary_key_table(),
            Some(&fk::name("main", "parent"))
        );
        let oracle = ::rusqlite::Connection::open_in_memory()?;
        oracle.execute_batch("CREATE TABLE Parent(id PRIMARY KEY); CREATE TABLE child(id REFERENCES PARENT(id)); ALTER TABLE pArEnT RENAME TO renamed;")?;
        let target: String = oracle.query_row(
            r#"SELECT "table" FROM pragma_foreign_key_list('child')"#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(target, "renamed");
        Ok(())
    }

    #[rstest::rstest]
    #[case::explicit("a.child(id)")]
    #[case::unqualified("child(id)")]
    #[case::omitted_columns("child")]
    fn create_table_self_reference_uses_in_progress_keys(#[case] reference: &str) {
        let context = fk::apply(
            fk::context(),
            &format!(
                "CREATE TABLE a.child(id integer, parent_id integer, CONSTRAINT fk FOREIGN KEY(parent_id) REFERENCES {reference}, PRIMARY KEY(id))"
            ),
        );
        let child = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(constraint.target_table(), Some(&fk::name("a", "child")));
        assert_eq!(
            constraint.target_columns(),
            Some([ColumnName::from("id")].as_slice())
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
    }

    #[test]
    fn in_progress_self_reference_respects_an_earlier_search_path_relation() {
        let context = fk::apply(
            fk::context(),
            "CREATE TABLE a.child(id integer PRIMARY KEY)",
        );
        let context = fk::apply(
            context,
            "CREATE TABLE b.child(id integer PRIMARY KEY, parent_id integer, CONSTRAINT fk FOREIGN KEY(parent_id) REFERENCES child(id))",
        );
        let child = context
            .get_table_impl(fk::name("b", "child").full_name())
            .unwrap();
        assert_eq!(
            child
                .constraint("fk")
                .unwrap()
                .as_foreign_key()
                .unwrap()
                .target_table(),
            Some(&fk::name("a", "child"))
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::main("main", "temp")]
    #[case::temporary("temp", "main")]
    fn sqlite_foreign_key_uses_its_own_database(
        #[case] own: &str,
        #[case] other: &str,
    ) -> testresult::TestResult {
        let context = fk::apply_sqlite(
            fk::sqlite_context(),
            "CREATE TABLE main.parent(id PRIMARY KEY)",
        );
        let context = fk::apply_sqlite(context, "CREATE TEMP TABLE parent(id PRIMARY KEY)");
        let sql = format!(
            "CREATE TABLE {own}.child(id, CONSTRAINT fk FOREIGN KEY(id) REFERENCES parent(id))"
        );
        let context = fk::apply_sqlite(context, &sql);
        let child_name = fk::name(own, "child");
        let child = context.get_table_impl(child_name.full_name()).unwrap();
        assert_eq!(
            child
                .constraint("fk")
                .unwrap()
                .as_foreign_key()
                .unwrap()
                .target_table(),
            Some(&fk::name(own, "parent"))
        );
        let rename = format!("ALTER TABLE {other}.parent RENAME TO unrelated");
        let context = fk::apply_sqlite(context, &rename);
        let child = context.get_table_impl(child_name.full_name()).unwrap();
        assert_eq!(
            child
                .constraint("fk")
                .unwrap()
                .as_foreign_key()
                .unwrap()
                .target_table(),
            Some(&fk::name(own, "parent"))
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let oracle = ::rusqlite::Connection::open_in_memory()?;
        oracle.execute_batch(
            "CREATE TABLE main.parent(id PRIMARY KEY); CREATE TEMP TABLE parent(id PRIMARY KEY)",
        )?;
        oracle.execute_batch(&sql)?;
        oracle.execute_batch(&rename)?;
        let target: String = oracle.query_row(
            r#"SELECT "table" FROM pragma_foreign_key_list('child', ?1)"#,
            [own],
            |row| row.get(0),
        )?;
        assert_eq!(target, "parent");
        Ok(())
    }
}
