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

impl From<&crate::syn::DefElem> for DefElemKey {
    fn from(value: &crate::syn::DefElem) -> Self {
        Self::new(&value.get_defnamespace(), &value.get_defname())
    }
}

/// hash map of `DefElem`
#[derive(Debug, Clone, Default)]
pub struct DefElemList(HashMap<DefElemKey, crate::syn::Node>);

impl DefElemList {
    /// test key exists
    #[must_use]
    pub fn contains(&self, key: &DefElemKey) -> bool {
        self.0.contains_key(key)
    }

    /// get key value
    #[must_use]
    pub fn get(&self, key: &DefElemKey) -> Option<&crate::syn::Node> {
        self.0.get(key)
    }
}

/// analyze [`crate::syn::NodeListOpt`] as [`DefElemList`]
pub fn analyze_def_elem_list<TParseContext>(
    mut context: TParseContext,
    syn: crate::syn::NodeListOpt,
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
