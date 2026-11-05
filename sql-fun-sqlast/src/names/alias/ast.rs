use crate::{ast::Name, names::AliasName, sem::AnalysisError};

impl AliasName {
    pub(crate) fn from_ast_star_qualifier(ast: &Name) -> Result<Option<Self>, AnalysisError> {
        match ast.0.as_slice() {
            [] => Ok(None),
            [alias] => Ok(Some(Self::from(alias.as_str()))),
            _ => AnalysisError::raise_unexpected_input("star qualification"),
        }
    }
}
