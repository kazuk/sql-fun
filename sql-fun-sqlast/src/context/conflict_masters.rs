//! Loaded collation and operator-class master catalogs.

use crate::{
    names::FullName,
    sem::{
        AnalysisError, CatalogNameLookup, CollationDefinition, DatabaseCollation,
        OperatorClassDefinition,
    },
};
use std::{collections::HashSet, fs::File, io::BufReader, path::Path};

#[derive(Debug, serde::Deserialize)]
struct CollationPayload {
    database: DatabaseCollation,
    collations: Vec<CollationDefinition>,
}

/// Loaded master definitions. Lookup never opens files or database connections.
#[derive(Debug, Clone)]
pub(crate) struct ConflictMasters {
    database: Option<DatabaseCollation>,
    collations: Vec<CollationDefinition>,
    operator_classes: Vec<OperatorClassDefinition>,
    sqlite_collations: Vec<String>,
}

impl ConflictMasters {
    /// Construct a file-free SQLite namespace.
    pub const fn sqlite() -> Self {
        Self {
            sqlite_collations: Vec::new(),
            database: None,
            collations: Vec::new(),
            operator_classes: Vec::new(),
        }
    }

    /// Load the two explicitly selected PostgreSQL master payloads.
    ///
    /// # Errors
    /// Returns an artifact-specific error for missing/malformed payloads or invalid
    /// identities, encoding, default provenance, and class/family relationships.
    pub fn load(collations: &Path, classes: &Path) -> Result<Self, AnalysisError> {
        let payload: CollationPayload = Self::read(collations)?;
        let operator_classes = Self::read(classes)?;
        let result = Self {
            database: Some(payload.database),
            collations: payload.collations,
            operator_classes,
            sqlite_collations: Vec::new(),
        };
        result.validate().map_err(|error| {
            AnalysisError::unexpected_input_error(&format!(
                "invalid masters {} / {}: {error}",
                collations.display(),
                classes.display()
            ))
        })?;
        Ok(result)
    }
}

#[cfg(test)]
mod test_load {

    use crate::test_helpers::conflict_masters as test_helpers;

    #[rstest::rstest]
    #[case::duplicate_collation("duplicate_collation")]
    #[case::duplicate_class("duplicate_class")]
    #[case::empty_classes("empty_classes")]
    #[case::encoding("encoding")]
    #[case::default_identity("default_identity")]
    #[case::default_provider("default_provider")]
    #[case::family_method("family_method")]
    #[case::malformed("malformed")]
    #[case::missing("missing")]
    fn rejects_invalid_artifacts_with_paths(#[case] defect: &str) {
        let mut fixture = test_helpers::Masters::new();
        fixture.corrupt(defect);
        let error = fixture.load().unwrap_err().to_string();
        assert!(
            error.contains("collations.json") || error.contains("operator_classes.json"),
            "{error}"
        );
    }
}

impl ConflictMasters {
    pub(crate) fn type_ordered(&self, name: &FullName) -> bool {
        self.operator_classes.iter().any(|class| {
            class.default()
                && class.access_method() == "btree"
                && class.input_type().qualified_name() == *name
        })
    }

    fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AnalysisError> {
        let read = || -> Result<T, Box<dyn std::error::Error>> {
            Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
        };
        read().map_err(|error| {
            AnalysisError::unexpected_input_error(&format!("{}: {error}", path.display()))
        })
    }

    fn validate(&self) -> Result<(), &'static str> {
        let database = self
            .database
            .as_ref()
            .ok_or("missing database provenance")?;
        if !(0..=34).contains(&database.encoding()) || database.encoding_name().is_empty() {
            return Err("invalid database encoding");
        }
        let mut identities = HashSet::new();
        let mut has_default = false;
        for collation in &self.collations {
            if collation.schema().is_empty()
                || collation.name().is_empty()
                || !(-1..=34).contains(&collation.encoding())
                || !["b", "c", "d", "i"].contains(&collation.provider())
            {
                return Err("invalid collation identity/encoding");
            }
            if !identities.insert((collation.schema(), collation.name(), collation.encoding())) {
                return Err("duplicate collation identity");
            }
            let identity = database.default_collation();
            if collation.schema() == identity.schema()
                && collation.name() == identity.name()
                && collation.encoding() == identity.encoding()
            {
                if collation.provider() != "d"
                    || !(collation.encoding() == -1 || collation.encoding() == database.encoding())
                {
                    return Err("invalid default collation provenance");
                }
                has_default = true;
            }
        }
        if !has_default {
            return Err("missing default collation definition");
        }
        if self.operator_classes.is_empty() {
            return Err("missing operator-class definitions");
        }
        let mut classes = HashSet::new();
        for class in &self.operator_classes {
            if class.schema().is_empty()
                || class.name().is_empty()
                || class.access_method().is_empty()
                || class.family().schema().is_empty()
                || class.family().name().is_empty()
                || class.input_type().schema().is_empty()
                || class.input_type().name().is_empty()
                || class
                    .key_type()
                    .as_ref()
                    .is_some_and(|key| key.schema().is_empty() || key.name().is_empty())
            {
                return Err("invalid operator-class identity/reference");
            }
            if class.access_method() != class.family().access_method() {
                return Err("operator-family access-method mismatch");
            }
            if !classes.insert((class.schema(), class.name(), class.access_method())) {
                return Err("duplicate operator-class identity");
            }
        }
        Ok(())
    }

    /// Source database provenance, absent for a SQLite connection namespace.
    pub const fn database(&self) -> Option<&DatabaseCollation> {
        self.database.as_ref()
    }

    /// Read the encoding-eligible definition at an exact qualified name.
    /// Encoding-specific definitions precede encoding-independent definitions.
    pub fn qualified_collation(&self, name: &FullName) -> Option<&CollationDefinition> {
        let encoding = self.database.as_ref()?.encoding();
        if let Some(specific) = self
            .collations
            .iter()
            .find(|value| value.matches_name(name) && value.encoding() == encoding)
        {
            return Some(specific);
        }
        self.collations.iter().find(|value| {
            value.matches_name(name)
                && value.encoding() == -1
                // PostgreSQL's stable encoding IDs: ICU cannot use SQL_ASCII,
                // EUC_JIS_2004, MULE_INTERNAL, LATIN10, or WIN874.
                && (value.provider() != "i" || ![0, 5, 7, 17, 21].contains(&encoding))
        })
    }
}

#[cfg(test)]
mod test_qualified_collation {

    use crate::{
        context::ConflictInfoRead, names::FullName, sem::CollationIdentity,
        test_helpers::conflict_masters as test_helpers,
    };

    #[rstest::rstest]
    #[case::specific(6, 6)]
    #[case::independent(8, -1)]
    fn preserves_encoding_identity_and_precedence(
        #[case] encoding: i32,
        #[case] selected_encoding: i32,
    ) {
        let mut fixture = test_helpers::Masters::new();
        fixture.set_encoding(encoding);
        fixture.add_collation("public", "variant", -1, "c");
        fixture.add_collation("public", "variant", 6, "c");
        let context = fixture.context();
        let name = FullName::with_schema("public", "variant");
        let definition = context.collation_definition(&name).unwrap();
        assert_eq!(definition.encoding(), selected_encoding);
        let identity = CollationIdentity::new(&name, definition.encoding()).unwrap();
        assert_eq!(identity.qualified_name(), definition.qualified_name());
        assert_eq!(identity.encoding(), definition.encoding());
        assert_eq!(context.database_collation().unwrap().encoding(), encoding);
    }

    #[rstest::rstest]
    #[case::utf8(6, true)]
    #[case::sql_ascii(0, false)]
    #[case::euc_jis_2004(5, false)]
    #[case::latin10(17, false)]
    #[case::win874(21, false)]
    fn filters_icu_encoding_eligibility(#[case] encoding: i32, #[case] found: bool) {
        let mut fixture = test_helpers::Masters::new();
        fixture.set_encoding(encoding);
        fixture.add_collation("public", "icu", -1, "i");
        assert_eq!(
            fixture
                .context()
                .collation_definition(&FullName::with_schema("public", "icu"))
                .is_some(),
            found
        );
    }
}

impl ConflictMasters {
    /// Read an exact operator-class identity without selecting a default class.
    pub fn operator_class(
        &self,
        name: &FullName,
        access_method: &str,
    ) -> Option<&OperatorClassDefinition> {
        self.operator_classes
            .iter()
            .find(|value| value.matches_name(name) && value.access_method() == access_method)
    }

    /// Declare an explicitly supplied connection-local name, including an empty name.
    /// This records availability; it does not register a runtime comparison callback.
    ///
    /// # Errors
    /// Returns an error for a PostgreSQL catalog or a NUL-containing name.
    pub fn declare_sqlite_collation(&mut self, name: &str) -> Result<(), AnalysisError> {
        if self.database.is_some() || name.contains('\0') {
            return Err(AnalysisError::unexpected_input_error(
                "a valid collation name and SQLite catalog are required",
            ));
        }
        if !self
            .sqlite_collations
            .iter()
            .any(|value| value.eq_ignore_ascii_case(name))
        {
            self.sqlite_collations.push(name.to_owned());
        }
        Ok(())
    }

    pub(crate) fn sqlite_lookup(&self, name: &FullName) -> CatalogNameLookup {
        if name.schema().is_some() || name.catalog().is_some() {
            return CatalogNameLookup::missing(name);
        }
        // Builtins belong to the active SQLite namespace even when the shared
        // catalog was originally loaded with PostgreSQL analysis metadata.
        for builtin in ["BINARY", "NOCASE", "RTRIM"] {
            if builtin.eq_ignore_ascii_case(name.name().as_str()) {
                return CatalogNameLookup::found(name, &FullName::from_local_name(builtin));
            }
        }
        if let Some(canonical) = self
            .sqlite_collations
            .iter()
            .find(|value| value.eq_ignore_ascii_case(name.name().as_str()))
        {
            CatalogNameLookup::found(name, &FullName::from_local_name(canonical))
        } else {
            CatalogNameLookup::missing(name)
        }
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests;
