use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use crate::SqlFunCodegenError;

/// cache Json like file content as `serde_json::Value`
pub struct JsonValueCache {
    entries: HashMap<PathBuf, serde_json::Value>,
}

static JSON_VALUES: LazyLock<Mutex<JsonValueCache>> =
    LazyLock::new(|| Mutex::new(JsonValueCache::default()));

impl JsonValueCache {
    fn default() -> Self {
        Self {
            entries: HashMap::default(),
        }
    }

    /// access file content
    pub fn get_or_fill<P, F, T, E>(path: P, handler: F) -> Result<T, SqlFunCodegenError>
    where
        P: AsRef<Path>,
        F: Fn(&serde_json::Value) -> Result<T, E>,
        E: Into<SqlFunCodegenError>,
    {
        let path = path.as_ref();

        let mut values = JSON_VALUES
            .lock()
            .map_err(|_| SqlFunCodegenError::JsonValueCachePoisoned())?;
        match values.entries.entry(path.to_path_buf()) {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                Ok(handler(occupied_entry.get()).map_err(std::convert::Into::into)?)
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                if !path.exists() {
                    return SqlFunCodegenError::file_not_found(path);
                }
                let value = Self::load(path)?;
                vacant_entry.insert(value.clone());
                Ok(handler(&value).map_err(std::convert::Into::into)?)
            }
        }
    }

    fn load(path: &Path) -> Result<serde_json::Value, SqlFunCodegenError> {
        let Some(ext) = path.extension() else {
            return SqlFunCodegenError::file_extension_not_found(path);
        };
        if ext == "json" {
            let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            Ok(value)
        } else if ext == "toml" {
            let value: serde_json::Value = toml::from_str(&std::fs::read_to_string(path)?)?;
            Ok(value)
        } else if ext == "yaml" {
            let value: serde_json::Value = serde_saphyr::from_str(&std::fs::read_to_string(path)?)?;
            Ok(value)
        } else {
            SqlFunCodegenError::file_type_not_supported(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JsonValueCache;

    #[test]
    fn yaml_cache_preserves_aliases_and_scalar_types() -> testresult::TestResult {
        let file = test_helpers::yaml_file(false)?;
        let load = || {
            JsonValueCache::get_or_fill(file.path(), |value| {
                Ok::<_, crate::SqlFunCodegenError>(value.clone())
            })
        };
        let value = load()?;
        assert_eq!(value["original"], value["alias"]);
        assert_eq!(value["original"]["count"], 3);
        assert_eq!(value["original"]["enabled"], true);
        std::fs::remove_file(file.path())?;
        assert_eq!(load()?, value);
        Ok(())
    }

    #[test]
    fn malformed_yaml_is_rejected() -> testresult::TestResult {
        let file = test_helpers::yaml_file(true)?;
        let result = JsonValueCache::get_or_fill(file.path(), |value| {
            Ok::<_, crate::SqlFunCodegenError>(value.clone())
        });
        assert!(result.is_err());
        Ok(())
    }

    mod test_helpers {
        pub(super) fn yaml_file(
            malformed: bool,
        ) -> testresult::TestResult<::tempfile::NamedTempFile> {
            let file = ::tempfile::NamedTempFile::with_suffix(".yaml")?;
            let text = if malformed {
                "original: [unfinished"
            } else {
                r"original: &original
  count: 3
  enabled: true
alias: *original
"
            };
            std::fs::write(file.path(), text)?;
            Ok(file)
        }
    }
}
