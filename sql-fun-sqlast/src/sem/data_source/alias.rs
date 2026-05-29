use itertools::Itertools;

use crate::{
    sem::{AnalysisError, ColumnName, FullName, ParseContext},
    syn::ListOpt,
};

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
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// alias name in from clause
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AliasSpec {
    alias_name: AliasName,
    derived_column_names: Vec<ColumnName>,
}

impl AliasSpec {
    /// get alias name
    #[must_use]
    pub fn name(&self) -> &AliasName {
        &self.alias_name
    }

    /// create alias spec from only local name
    #[must_use]
    pub fn from_local_name(local_name: &str) -> Self {
        Self {
            alias_name: AliasName(String::from(local_name)),
            derived_column_names: Vec::new(),
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
    pub fn len_derived_column(&self) -> usize {
        self.derived_column_names.len()
    }
}
