use crate::sem::AliasSpec;
use ::sql_fun_core::LookupResult;

/// Known candidates and whether additional candidates remain possible.
#[derive(Debug, Default)]
pub(crate) struct ColumnCandidates {
    aliases: Vec<AliasSpec>,
    unknown: bool,
}

impl From<Vec<AliasSpec>> for ColumnCandidates {
    fn from(aliases: Vec<AliasSpec>) -> Self {
        Self {
            aliases,
            unknown: false,
        }
    }
}

impl ColumnCandidates {
    pub(crate) const fn unknown() -> Self {
        Self {
            aliases: Vec::new(),
            unknown: true,
        }
    }

    pub(crate) fn append(&mut self, mut other: Self) {
        self.aliases.append(&mut other.aliases);
        self.unknown |= other.unknown;
    }

    pub(crate) const fn is_unknown(&self) -> bool {
        self.unknown
    }

    pub(crate) const fn is_empty(&self) -> bool {
        !self.unknown && self.aliases.is_empty()
    }

    pub(crate) fn with_group_alias(mut self, alias: &AliasSpec) -> Self {
        if !self.aliases.is_empty() {
            self.aliases = vec![alias.clone()];
        }
        self
    }

    pub(crate) fn into_lookup(self) -> LookupResult<Vec<AliasSpec>> {
        if self.unknown {
            LookupResult::unknown()
        } else if self.aliases.is_empty() {
            LookupResult::not_found()
        } else {
            LookupResult::found(self.aliases)
        }
    }

    pub(crate) fn resolve(mut self) -> Result<LookupResult<AliasSpec>, Vec<AliasSpec>> {
        if self.aliases.len() > 1 {
            Err(self.aliases)
        } else if self.unknown {
            Ok(LookupResult::unknown())
        } else if let Some(alias) = self.aliases.pop() {
            Ok(LookupResult::found(alias))
        } else {
            Ok(LookupResult::not_found())
        }
    }
}
