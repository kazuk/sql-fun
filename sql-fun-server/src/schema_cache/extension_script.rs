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
    if !settings.relocatable && sql.contains("@extschema@") {
        sql = sql.replace("@extschema@", &substitution_identifier(schema)?);
    }
    let mut search_path = vec![schema.to_owned()];
    for name in required_extensions(&settings.requires)? {
        let token = format!("@extschema:{name}@");
        if let Some(schema) = required_schemas.get(&name) {
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
    if let Some(pathname) = &settings.module_pathname {
        sql = sql.replace("MODULE_PATHNAME", pathname);
    }
    search_path.push("pg_temp".to_owned());
    Ok((sql, search_path))
}

#[cfg(test)]
mod tests {
    use crate::schema_cache::extension_script::prepare;
    use sql_fun_core::ExtensionScriptSettings;
    use std::collections::HashMap;

    #[test]
    fn replaces_script_text_in_postgres_order() {
        let settings = ExtensionScriptSettings {
            requires: r#"CuBe, "Other Extension""#.to_owned(),
            module_pathname: Some("$libdir/example".to_owned()),
            ..ExtensionScriptSettings::default()
        };
        let dependencies = HashMap::from([
            ("cube".to_owned(), "Cube Schema".to_owned()),
            ("Other Extension".to_owned(), "other".to_owned()),
        ]);
        let (sql, path) = prepare(
            "\\echo guard\nSELECT '@extschema@ @extschema:cube@ @extschema:Other Extension@ @extowner@ MODULE_PATHNAME';",
            &settings, "select", Some("public"), &dependencies,
        ).unwrap();
        assert_eq!(
            sql,
            "\nSELECT 'public \"Cube Schema\" other \"select\" $libdir/example';"
        );
        assert_eq!(path, ["public", "Cube Schema", "other", "pg_temp"]);
    }

    #[test]
    fn relocatable_scripts_still_replace_dependency_schemas() {
        let settings = ExtensionScriptSettings {
            relocatable: true,
            requires: "cube".to_owned(),
            ..ExtensionScriptSettings::default()
        };
        let dependencies = HashMap::from([("cube".to_owned(), "public".to_owned())]);
        let text = "SELECT '@extschema@ @extschema:cube@ @extowner@ MODULE_PATHNAME';";
        let (sql, _) = prepare(text, &settings, "postgres", Some("public"), &dependencies).unwrap();
        assert_eq!(sql, "SELECT '@extschema@ public postgres MODULE_PATHNAME';");
        assert!(prepare(text, &settings, "postgres", Some("public"), &HashMap::new()).is_err());
        assert!(prepare(text, &settings, "bad$owner", Some("public"), &dependencies).is_err());
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

fn required_extensions(input: &str) -> Result<Vec<String>, SchemaLoadError> {
    let mut names = Vec::new();
    let mut rest = input.trim();
    while !rest.is_empty() {
        let mut name = String::new();
        if let Some(quoted) = rest.strip_prefix('"') {
            rest = quoted;
            loop {
                let Some((part, tail)) = rest.split_once('"') else {
                    return Err(SchemaLoadError::invalid(
                        "unterminated quoted extension name in requires",
                    ));
                };
                name.push_str(part);
                if let Some(tail) = tail.strip_prefix('"') {
                    name.push('"');
                    rest = tail;
                } else {
                    rest = tail;
                    break;
                }
            }
        } else {
            let end = rest
                .find(|ch: char| ch == ',' || ch.is_ascii_whitespace())
                .unwrap_or(rest.len());
            name = rest[..end].to_ascii_lowercase();
            rest = &rest[end..];
        }
        names.push(name);
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let Some(tail) = rest.strip_prefix(',') else {
            return Err(SchemaLoadError::invalid(
                "expected comma in extension requires list",
            ));
        };
        rest = tail.trim_start();
        if rest.is_empty() {
            return Err(SchemaLoadError::invalid(
                "missing name in extension requires list",
            ));
        }
    }
    if names.iter().any(String::is_empty) {
        return Err(SchemaLoadError::invalid(
            "empty name in extension requires list",
        ));
    }
    Ok(names)
}
