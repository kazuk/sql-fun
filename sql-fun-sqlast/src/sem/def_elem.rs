use crate::syn::{DefElem, Node, NodeListOpt};
use std::collections::HashMap;

use crate::{
    sem::{AnalysisError, AnalysisProblem, ParseContext},
    syn::ListOpt,
};

/// key of `DefElem`
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct DefElemKey {
    namespace: String,
    name: String,
}

impl DefElemKey {
    fn new(namespace: &str, name: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            name: name.to_string(),
        }
    }

    /// strict `DefElem` name
    #[must_use]
    pub fn strict() -> Self {
        Self::new("", "strict")
    }
}

impl std::fmt::Display for DefElemKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.namespace.is_empty() {
            write!(
                f,
                "{}.",
                crate::identifier::CanonicalIdentifier::from(self.namespace.as_str())
            )?;
        }
        write!(
            f,
            "{}",
            crate::identifier::CanonicalIdentifier::from(self.name.as_str())
        )
    }
}

#[cfg(test)]
mod display_tests {
    use crate::sem::def_elem::DefElemKey;

    #[rstest::rstest]
    #[case::local("", "strict", "strict")]
    #[case::qualified("storage", "enabled", "storage.enabled")]
    #[case::escaped("a.b", "a\"b", r#""a.b"."a""b""#)]
    fn presents_logical_option_components(
        #[case] namespace: &str,
        #[case] name: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(DefElemKey::new(namespace, name).to_string(), expected);
    }
}

impl From<&DefElem> for DefElemKey {
    fn from(value: &DefElem) -> Self {
        Self::new(&value.get_defnamespace(), &value.get_defname())
    }
}

/// hash map of `DefElem`
#[derive(Debug, Clone, Default)]
pub struct DefElemList(HashMap<DefElemKey, Node>);

impl DefElemList {
    /// test key exists
    #[must_use]
    pub fn contains(&self, key: &DefElemKey) -> bool {
        self.0.contains_key(key)
    }

    /// get key value
    #[must_use]
    pub fn get(&self, key: &DefElemKey) -> Option<&Node> {
        self.0.get(key)
    }
}

/// analyze [`crate::syn::NodeListOpt`] as [`DefElemList`]
pub fn analyze_def_elem_list<TParseContext>(
    mut context: TParseContext,
    syn: NodeListOpt,
) -> Result<(DefElemList, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(def_elem_list) = syn.map(|item| item.as_def_elem()) else {
        return Ok((DefElemList::default(), context));
    };

    let mut items = HashMap::default();

    for def_elem in def_elem_list {
        let key = DefElemKey::from(&def_elem);
        let arg = def_elem.get_arg();
        if items.contains_key(&key) {
            context.report_analysis_problem(AnalysisProblem::duplicate_def_elem_key(&key))?;
        }
        items.insert(key, arg);
    }
    Ok((DefElemList(items), context))
}
