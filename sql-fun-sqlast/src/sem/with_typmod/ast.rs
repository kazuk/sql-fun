use crate::{
    ast::{Dialect, TypeName},
    sem::{AnalysisError, TypeReference, WithTypmod},
};

impl WithTypmod<TypeReference> {
    pub(crate) fn from_ast_declaration(
        ast: &TypeName,
        resolved: Option<&TypeReference>,
    ) -> Result<Self, AnalysisError> {
        let mut declaration = Self::new(
            &TypeReference::from_ast(ast)?,
            &Self::ast_modifier_arguments(ast)?,
        );
        if let Some(resolved) = resolved {
            declaration.interpret_postgres(resolved);
        }
        Ok(declaration)
    }
}

impl WithTypmod<TypeReference> {
    pub(crate) fn ast_modifier_arguments(ast: &TypeName) -> Result<Vec<String>, AnalysisError> {
        use crate::ast::deparse::{SqlEmit, SqlWriter};
        let mut arguments = Vec::new();
        for modifier in &ast.modifiers {
            let mut writer = SqlWriter::new(Dialect::postgres());
            modifier
                .emit(&mut writer)
                .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
            arguments.push(writer.into_sql());
        }
        Ok(arguments)
    }
}
