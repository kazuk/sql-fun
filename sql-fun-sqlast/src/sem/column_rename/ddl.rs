//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::AlterCommand as AstAlterCommand,
    names::ColumnName,
    sem::{AnalysisError, ColumnRename},
};

impl ColumnRename {
    pub(crate) const fn is_ast_command(ast: &AstAlterCommand) -> bool {
        matches!(ast, AstAlterCommand::RenameColumn(_, _))
    }

    pub(crate) fn names_from_ast(
        ast: &AstAlterCommand,
    ) -> Result<(ColumnName, ColumnName), AnalysisError> {
        let AstAlterCommand::RenameColumn(from, to) = ast else {
            return AnalysisError::raise_unexpected_input("column rename command");
        };
        Ok((
            ColumnName::from(from.as_str()),
            ColumnName::from(to.as_str()),
        ))
    }
}
