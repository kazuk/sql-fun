use std::collections::BTreeSet;

/// Explicit additional master-definition scope, independent of builtin schema extraction.
pub(super) struct DefinitionSchemas(BTreeSet<String>);

impl DefinitionSchemas {
    pub(super) fn new(names: &[String]) -> Result<Self, anyhow::Error> {
        let mut schemas = BTreeSet::new();
        for name in names {
            anyhow::ensure!(
                !name.is_empty() && !name.contains('\0'),
                "definition schema must be a nonempty PostgreSQL identifier without NUL"
            );
            schemas.insert(name.clone());
        }
        Ok(Self(schemas))
    }

    pub(super) fn session_sql(&self) -> Result<String, anyhow::Error> {
        // Standard-conforming strings are enabled before this literal is evaluated.
        // JSON preserves exact identifier strings; SQL quoting prevents injection.
        let names = serde_json::to_string(&self.0)?.replace('\'', "''");
        Ok(format!(
            "SET LOCAL sql_fun.definition_schemas = '{names}';\n\
             SELECT (NOT EXISTS (\
             SELECT 1 FROM jsonb_array_elements_text(current_setting('sql_fun.definition_schemas')::jsonb) AS selected(name) \
             WHERE NOT EXISTS (SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname::text = selected.name)\
             ))::text;\n"
        ))
    }

    pub(super) fn record_scope(&self, metadata: &mut serde_json::Value) {
        if !self.0.is_empty() {
            metadata["definition_schemas"] = serde_json::json!(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DefinitionSchemas;

    #[test]
    fn scope_is_explicit_sorted_and_deduplicated() {
        let schemas = DefinitionSchemas::new(&["z".into(), "a".into(), "z".into()]).unwrap();
        let mut metadata = serde_json::json!({"format_version": 1});
        schemas.record_scope(&mut metadata);
        assert_eq!(
            metadata["definition_schemas"],
            serde_json::json!(["a", "z"])
        );
        DefinitionSchemas::new(&[])
            .unwrap()
            .record_scope(&mut metadata);
        assert_eq!(metadata["format_version"], 1);
        let mut builtin = serde_json::json!({});
        DefinitionSchemas::new(&[])
            .unwrap()
            .record_scope(&mut builtin);
        assert!(builtin.get("definition_schemas").is_none());
    }

    #[rstest::rstest]
    #[case::empty("")]
    #[case::nul("a\0b")]
    fn rejects_invalid_identifiers(#[case] name: &str) {
        assert!(DefinitionSchemas::new(&[name.into()]).is_err());
    }

    #[test]
    fn quotes_schema_values_without_changing_identifiers() {
        let name = r#"Odd'\"; SELECT 1; -- 雪"#;
        let sql = DefinitionSchemas::new(&[name.into()])
            .unwrap()
            .session_sql()
            .unwrap();
        pg_query::parse(&sql).unwrap();
        assert!(sql.contains("Odd''"));
        assert!(sql.contains("雪"));
    }
}
