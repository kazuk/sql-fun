use crate::{ast::Name, names::FullName, sem::AnalysisError};

impl FullName {
    pub(crate) fn from_optional_ast_name(ast: &Name) -> Result<Option<Self>, AnalysisError> {
        if ast.0.is_empty() {
            Ok(None)
        } else {
            Self::from_ast_name(ast).map(Some)
        }
    }
}

impl FullName {
    pub(crate) fn from_ast_star_relation(ast: &Name) -> Result<Option<Self>, AnalysisError> {
        if ast.0.len() < 2 {
            Ok(None)
        } else {
            Self::from_ast_name(ast).map(Some)
        }
    }
}

impl FullName {
    pub(crate) fn from_ast_name(ast: &Name) -> Result<Self, AnalysisError> {
        match ast.0.as_slice() {
            [local] => Ok(Self::from_local_name(local)),
            [schema, local] => Ok(Self::with_schema(schema, local)),
            [catalog, schema, local] => Ok(Self::with_catalog(catalog, schema, local)),
            _ => AnalysisError::raise_unexpected_input("qualified name component count"),
        }
    }
}
