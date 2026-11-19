use std::collections::HashMap;

use sql_fun_core::ExtensionScriptSettings;
use sql_fun_sqlast::{
    PostgresDialect,
    syn::{KeywordKind, Opt},
};

use crate::SchemaLoadError;

pub(super) fn prepare(
    text: &str,
    settings: &ExtensionScriptSettings,
    owner: &str,
    schema: Option<&str>,
    required_schemas: &HashMap<String, String>,
) -> Result<(String, Vec<String>), SchemaLoadError> {
    let schema =
        schema.ok_or_else(|| SchemaLoadError::invalid("missing extension installation schema"))?;
    let mut sql = text
        .split_inclusive('\n')
        .map(|line| {
            if line.starts_with("\\echo") {
                if line.ends_with('\n') { "\n" } else { "" }
            } else {
                line
            }
        })
        .collect::<String>();
    // PostgreSQL checks the original script for the owner token.
    if text.contains("@extowner@") {
        sql = sql.replace("@extowner@", &substitution_identifier(owner)?);
    }
    if !settings.relocatable() && sql.contains("@extschema@") {
        sql = sql.replace("@extschema@", &substitution_identifier(schema)?);
    }
    let mut search_path = vec![schema.to_owned()];
    for name in settings.requires() {
        let token = format!("@extschema:{name}@");
        if let Some(schema) = required_schemas.get(name) {
            if sql.contains(&token) {
                sql = sql.replace(&token, &substitution_identifier(schema)?);
            }
            if schema != "pg_catalog" {
                search_path.push(schema.clone());
            }
        } else if sql.contains(&token) {
            return Err(SchemaLoadError::invalid(&format!(
                "missing installation schema for required extension {name}"
            )));
        }
    }
    if let Some(pathname) = settings.module_pathname() {
        sql = sql.replace("MODULE_PATHNAME", pathname);
    }
    search_path.push("pg_temp".to_owned());
    Ok((sql, search_path))
}

#[cfg(test)]
mod tests {
    use crate::{
        schema_cache::extension_script::prepare, test_helpers::extension_script_with_control,
    };
    use sql_fun_core::ExtensionScriptSettings;
    use std::collections::HashMap;

    #[test]
    fn replaces_script_text_in_postgres_order() {
        let script = extension_script_with_control(
            r#"requires = 'CuBe, "Other Extension"'
module_pathname = '$libdir/example'"#,
        );
        let dependencies = HashMap::from([
            ("cube".to_owned(), "Cube Schema".to_owned()),
            ("Other Extension".to_owned(), "other".to_owned()),
        ]);
        let (sql, path) = prepare(
            "\\echo guard\nSELECT '@extschema@ @extschema:cube@ @extschema:Other Extension@ @extowner@ MODULE_PATHNAME';",
            script.settings(), "select", Some("public"), &dependencies,
        ).unwrap();
        assert_eq!(
            sql,
            "\nSELECT 'public \"Cube Schema\" other \"select\" $libdir/example';"
        );
        assert_eq!(path, ["public", "Cube Schema", "other", "pg_temp"]);
    }

    #[test]
    fn relocatable_scripts_still_replace_dependency_schemas() {
        let script = extension_script_with_control("relocatable = true\nrequires = cube");
        let dependencies = HashMap::from([("cube".to_owned(), "public".to_owned())]);
        let text = "SELECT '@extschema@ @extschema:cube@ @extowner@ MODULE_PATHNAME';";
        let (sql, _) = prepare(
            text,
            script.settings(),
            "postgres",
            Some("public"),
            &dependencies,
        )
        .unwrap();
        assert_eq!(sql, "SELECT '@extschema@ public postgres MODULE_PATHNAME';");
        assert!(
            prepare(
                text,
                script.settings(),
                "postgres",
                Some("public"),
                &HashMap::new()
            )
            .is_err()
        );
        assert!(
            prepare(
                text,
                script.settings(),
                "bad$owner",
                Some("public"),
                &dependencies
            )
            .is_err()
        );
        assert!(
            prepare(
                "SELECT '@extschema@';",
                &ExtensionScriptSettings::default(),
                "postgres",
                None,
                &dependencies
            )
            .is_err()
        );
    }
}

fn substitution_identifier(name: &str) -> Result<String, SchemaLoadError> {
    if name.is_empty() || name.contains(['"', '$', '\'', '\\', '\0']) {
        return Err(SchemaLoadError::invalid(&format!(
            "invalid extension substitution identifier {name:?}"
        )));
    }
    let mut bytes = name.bytes();
    if matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
    {
        let tokens =
            PostgresDialect::scan(name).map_err(|error| SchemaLoadError::source(&error))?;
        if tokens.iter().next().is_some_and(|token| {
            matches!(
                token.get_keyword_kind().as_inner(),
                Some(KeywordKind::NoKeyword | KeywordKind::UnreservedKeyword)
            )
        }) {
            return Ok(name.to_owned());
        }
    }
    Ok(format!("\"{name}\""))
}
