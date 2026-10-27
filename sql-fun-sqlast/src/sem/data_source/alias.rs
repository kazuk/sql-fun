use crate::{
    sem::{AnalysisError, ColumnName, FullName, ParseContext},
    syn::ListOpt,
};

use ::itertools::Itertools;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AliasName(String);

impl std::fmt::Display for AliasName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for AliasName {
    fn from(value: &str) -> Self {
        Self(String::from(value))
    }
}

impl AliasName {
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// alias name in from clause
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AliasSpec {
    alias_name: AliasName,
    derived_column_names: Vec<ColumnName>,
    // Required on deserialization: an old binding cannot reconstruct alias hiding.
    explicit: bool,
}

impl AliasSpec {
    pub(super) fn projection_name(
        &self,
        index: usize,
        original: &crate::sem::ColumnName,
    ) -> crate::sem::ColumnName {
        self.derived_column_at(index).unwrap_or(original).clone()
    }

    pub(crate) fn from_names(name: &str, columns: &[String]) -> Self {
        let mut derived_column_names = Vec::new();
        for column in columns {
            derived_column_names.push(ColumnName::from(column.as_str()));
        }
        Self {
            alias_name: AliasName::from(name),
            derived_column_names,
            explicit: true,
        }
    }

    pub(crate) fn derived_column_at(&self, index: usize) -> Option<&ColumnName> {
        self.derived_column_names.get(index)
    }

    pub(crate) const fn is_explicit(&self) -> bool {
        self.explicit
    }

    /// get alias name
    #[must_use]
    pub const fn name(&self) -> &AliasName {
        &self.alias_name
    }

    /// Create the implicit binding name of an unaliased relation.
    #[must_use]
    pub fn from_local_name(local_name: &str) -> Self {
        Self {
            alias_name: AliasName(String::from(local_name)),
            derived_column_names: Vec::new(),
            explicit: false,
        }
    }

    /// analyze Alias syn node
    pub fn analyze<TParseContext>(
        context: TParseContext,
        rel_fullname: &FullName,
        alias: &crate::syn::Alias,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let alias_name = alias.get_aliasname();
        let alias_name = if alias_name.is_empty() {
            rel_fullname.local_name()
        } else {
            alias_name
        };
        let col_names = if let Some(col_names) = alias.get_colnames().map(|v| v.as_string()) {
            col_names
                .iter()
                .map(|v| ColumnName::from(v.get_sval()))
                .collect()
        } else {
            Vec::default()
        };
        Ok((
            Self {
                alias_name: AliasName(alias_name),
                derived_column_names: col_names,
                explicit: !alias.get_aliasname().is_empty(),
            },
            context,
        ))
    }

    /// analyze Alias syn node without default alias name
    pub fn analyze_without_relname<TParseContext>(
        context: TParseContext,
        alias: &crate::syn::Alias,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let alias_name = alias.get_aliasname();
        if alias_name.is_empty() {
            AnalysisError::raise_unexpected_none("alias.aliasname is empty")?;
        }
        let col_names = if let Some(col_names) = alias.get_colnames().map(|v| v.as_string()) {
            col_names
                .iter()
                .map(|v| ColumnName::from(v.get_sval()))
                .collect()
        } else {
            Vec::default()
        };
        Ok((
            Self {
                alias_name: AliasName(alias_name),
                derived_column_names: col_names,
                explicit: !alias.get_aliasname().is_empty(),
            },
            context,
        ))
    }

    /// find column name from delived column list
    #[must_use]
    pub fn derived_column_index(&self, column: &ColumnName) -> Option<usize> {
        if let Some((position, _)) = self
            .derived_column_names
            .iter()
            .find_position(|n| *n == column)
        {
            Some(position)
        } else {
            None
        }
    }

    /// get length of derived column list
    #[must_use]
    pub const fn len_derived_column(&self) -> usize {
        self.derived_column_names.len()
    }
}

#[cfg(test)]
mod test_alias_provenance {
    use super::AliasSpec;

    #[rstest::rstest]
    #[case::implicit(AliasSpec::from_local_name("t"), false)]
    #[case::explicit_same_name(AliasSpec::from_names("t", &[]), true)]
    fn serialization_retains_alias_hiding(#[case] alias: AliasSpec, #[case] explicit: bool) {
        let serialized = ::serde_json::to_string(&alias).unwrap();
        let restored: AliasSpec = ::serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.is_explicit(), explicit);
        assert_eq!(restored, alias);
    }

    #[test]
    fn missing_provenance_is_not_inferred() {
        assert!(
            ::serde_json::from_str::<AliasSpec>(r#"{"alias_name":"t","derived_column_names":[]}"#)
                .is_err()
        );
    }
}

impl AliasSpec {
    pub(crate) fn inference_columns(&self) -> Vec<String> {
        self.derived_column_names
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect()
    }
}
