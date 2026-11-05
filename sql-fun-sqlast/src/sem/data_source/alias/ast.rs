use crate::{ast::Alias, sem::AliasSpec};

impl AliasSpec {
    pub(crate) fn from_ast(ast: &Alias) -> Self {
        Self::from_names(&ast.name, &ast.columns)
    }
}
