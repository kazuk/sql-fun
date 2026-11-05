use crate::{
    ast::Join,
    names::ColumnName,
    sem::{AliasSpec, JoinUsingClause},
};

impl JoinUsingClause {
    pub(crate) fn ast_using_clause(ast: &Join) -> Option<Self> {
        if ast.using.is_empty() {
            return None;
        }
        let mut columns = Vec::new();
        for column in &ast.using {
            columns.push(ColumnName::from(column.as_str()));
        }
        Some(Self::new(
            &columns,
            &ast.using_alias.as_ref().map(AliasSpec::from_ast),
        ))
    }
}
